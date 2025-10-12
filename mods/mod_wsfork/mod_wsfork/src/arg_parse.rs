use std::{ffi::CString, net::SocketAddr, str::FromStr};

use anyhow::{Result, anyhow};
use clap::{Command, FromArgMatches as _, Parser, Subcommand as _, ValueEnum};
use http_body_util::Empty;
use hyper::{
    Method, Request,
    body::Bytes,
    header::{CONNECTION, UPGRADE},
};
use serde_json::Value;
use url::Url;

pub type WSRequest = Request<Empty<Bytes>>;

#[derive(Parser, Debug)]
pub(crate) struct Common {
    pub session_id: CString,
    pub fork_id: CString,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub(crate) enum AudioMix {
    Mono,
    Stereo,
    #[default]
    Mixed,
}

#[derive(Parser, Debug, Clone)]
pub(crate) struct Endpoint {
    pub url: Url,
    pub headers: String,
}

impl Endpoint {
    pub(crate) fn addr(&self) -> Result<SocketAddr> {
        self.url
            .socket_addrs(|| None)?
            .pop()
            .ok_or(anyhow!("failed to find any socket_addrs"))
    }

    pub(crate) fn to_request(&self) -> Result<WSRequest> {
        let mut req = Request::builder()
            .method(Method::GET.as_str())
            .uri(self.url.as_str())
            .header(UPGRADE, "websocket")
            .header(CONNECTION, "upgrade")
            .header(
                "Sec-WebSocket-Key",
                fastwebsockets::handshake::generate_key(),
            )
            .header("Sec-WebSocket-Version", "13");

        let headers = serde_json::Value::from_str(&self.headers)?;
        match headers {
            Value::Null => {}
            Value::String(s) if s.is_empty() => {}
            Value::Object(map) => {
                for (k, v) in map {
                    let Value::String(s) = v else {
                        return Err(anyhow!("Non String value found in header json"));
                    };
                    req = req.header(k, s);
                }
            }
            other => {
                return Err(anyhow!(
                    "Non supported header json: {other}, is_object:{}, is_string:{}, is_null:{}",
                    other.is_object(),
                    other.is_string(),
                    other.is_null(),
                ));
            }
        }

        req.body(Empty::new()).map_err(|e| e.into())
    }
}

#[derive(Parser, Debug)]
pub(crate) enum Subcommands {
    Start {
        #[command(flatten)]
        fork: Common,
        #[command(flatten)]
        endpoint: Endpoint,
        #[arg(value_enum)]
        mix: AudioMix,
        start_paused: Option<bool>,
    },
    Stop {
        #[command(flatten)]
        fork: Common,
    },
    Pause {
        #[command(flatten)]
        fork: Common,
    },
    Resume {
        #[command(flatten)]
        fork: Common,
    },
    SendText {
        #[command(flatten)]
        fork: Common,
        text: String,
    },
}

impl Subcommands {
    pub(crate) fn common_args(&self) -> &Common {
        match self {
            Self::SendText { fork, .. } => fork,
            Self::Start { fork, .. } => fork,
            Self::Stop { fork, .. } => fork,
            Self::Pause { fork, .. } => fork,
            Self::Resume { fork, .. } => fork,
        }
    }
}

pub(crate) fn parse_args(cmd_str: &str) -> Result<Subcommands> {
    let mut cmd = Command::new("wsfork")
        .disable_version_flag(true)
        .disable_help_flag(true)
        .ignore_errors(true)
        .no_binary_name(true);
    cmd = Subcommands::augment_subcommands(cmd);
    let matches = cmd.try_get_matches_from(cmd_str.split(' '))?;
    let s = Subcommands::from_arg_matches(&matches)?;
    Ok(s)
}
