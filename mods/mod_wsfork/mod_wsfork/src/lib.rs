mod audio_fork;

use fastwebsockets::handshake;
use hyper_util::rt::TokioExecutor;
use tokio::runtime;
use tokio::runtime::Runtime;
use wsfork_events::Body;
pub use wsfork_events::MOD_WSFORK_EVENT;
use wsfork_events::WSForkEvent;

use anyhow::Result;
use anyhow::anyhow;
use clap::{Command, FromArgMatches as _, Parser, Subcommand as _};

use freeswitch_rs::Session;
use freeswitch_rs::switch_status_t;
use std::ffi::CStr;
use std::ffi::CString;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::net::TcpStream;

use freeswitch_rs::log::{debug, error, info, warn};
use freeswitch_rs::*;

use crate::audio_fork::WSForkerError;
use crate::audio_fork::new_wsfork;

const BUG_FN_NAME: &CStr = c"MOD_WSFORK_BUG";

static RT: OnceLock<Runtime> = OnceLock::new();

struct Foobar {
    pub tx: audio_fork::WSForkSender,
}

#[derive(Parser, Debug)]
enum Subcommands {
    Start {
        #[arg()]
        session: String,
        #[arg()]
        url: String,
        #[arg(default_value_t)]
        bug_name: String,
    },
    Stop {
        #[arg()]
        session: String,
        #[arg(default_value_t)]
        bug_name: String,
    },
}

fn parse_args(cmd_str: &str) -> Result<Subcommands> {
    let mut cmd = Command::new("argparse")
        .disable_version_flag(true)
        .disable_help_flag(true)
        .no_binary_name(true);
    cmd = Subcommands::augment_subcommands(cmd);
    let matches = cmd.get_matches_from(cmd_str.split(' '));
    let s = Subcommands::from_arg_matches(&matches)?;
    Ok(s)
}

#[switch_module_define(mod_audiofork)]
struct FSMod;

impl LoadableModule for FSMod {
    fn load(module: FSModuleInterface, _pool: FSModulePool) -> switch_status_t {
        info!(channel=SWITCH_CHANNEL_ID_LOG; "mod ws_fork loading");
        // TODO: make worker count configurable
        //
        let Result::Ok(runtime) = runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(5)
            .build()
        else {
            return switch_status_t::SWITCH_STATUS_GENERR;
        };
        let _ = RT.set(runtime);

        module.add_api(api_main);

        if Event::reserve_subclass(MOD_WSFORK_EVENT).is_err() {
            error!(channel=SWITCH_CHANNEL_ID_LOG; "Failure to register custom events");
            return switch_status_t::SWITCH_STATUS_TERM;
        }

        switch_status_t::SWITCH_STATUS_SUCCESS
    }

    fn shutdown() -> switch_status_t {
        info!(channel=SWITCH_CHANNEL_ID_LOG; "mod ws_fork shutdown");
        let _ = Event::free_subclass(MOD_WSFORK_EVENT);
        switch_status_t::SWITCH_STATUS_SUCCESS
    }
}

#[switch_api_define("ws_fork")]
fn api_main(cmd: &str, _session: Option<&Session>, mut stream: StreamHandle) -> switch_status_t {
    debug!(channel=SWITCH_CHANNEL_ID_SESSION; "mod audiofork cmd {}", &cmd);
    match parse_args(cmd) {
        Err(_) => {
            let _ = write!(stream, "ERR: mod audiofork invalid usage");
        }
        Ok(cmd) => {
            let res = match cmd {
                Subcommands::Start {
                    session,
                    url,
                    bug_name,
                } => api_start(session, url, bug_name),
                Subcommands::Stop { session, bug_name } => api_stop(session, bug_name),
            };
            if let Err(e) = res {
                error!(channel=SWITCH_CHANNEL_ID_SESSION; "mod audiofork error: {}", &e);
            }
        }
    }
    switch_status_t::SWITCH_STATUS_SUCCESS
}

fn api_stop(session_id: String, bug_name: String) -> Result<()> {
    let key = CString::new(bug_name.clone())?;
    let (session, bug) = Session::locate(&session_id)
        .and_then(|s| unsafe {
            s.get_channel()
                .and_then(|c| c.get_private_with_key::<MediaBugHandle>(&key).cloned())
                .map(|b| (s, b))
        })
        .ok_or(anyhow!(
            "Unable to find bug {bug_name} for session {session_id}"
        ))?;
    session.remove_media_bug(bug)?;
    Ok(())
}

fn api_start(session_id: String, url: String, bug_name: String) -> Result<()> {
    debug!(channel=SWITCH_CHANNEL_ID_SESSION; "mod audiofork start uuid:{}",session_id);
    let session =
        Session::locate(&session_id).ok_or(anyhow!("Session Not Found: {}", session_id))?;

    let url = url::Url::parse(&url)?;
    let mut addrs = url.socket_addrs(|| None)?;
    let addr = addrs.pop().ok_or(anyhow!(""))?;

    let frame_size = 0;
    let buf_duration = Duration::from_millis(20);

    let (tx, rx) = new_wsfork(url.clone(), frame_size, buf_duration, |_| {})?;
    let owned = Arc::new(Foobar { tx });
    let weak_ref = Arc::downgrade(&owned);

    let bug = session.add_media_bug(
        Some(BUG_FN_NAME.into()),
        None,
        MediaBugFlags::SMBF_BOTH,
        move |bug, abc_type| {
            // For error handling, if we return false from closure
            // FS will prune mal functioning bug ( ie remove it )
            let Some(handle) = weak_ref.upgrade() else {
                return false;
            };
            match abc_type {
                switch_abc_type_t::SWITCH_ABC_TYPE_CLOSE => {
                    handle.tx.cancel();
                }
                switch_abc_type_t::SWITCH_ABC_TYPE_READ => {
                    match handle.tx.send_frame(bug) {
                        Err(WSForkerError::Full) => {
                            warn!(channel=SWITCH_CHANNEL_ID_LOG; "Buffer full + Packets dropped")
                        }
                        Err(WSForkerError::Closed) => {
                            // WS has closed, stop bug
                            debug!(channel=SWITCH_CHANNEL_ID_LOG; "WS Closed, Pruning bug");
                            return false;
                        }
                        Err(WSForkerError::ReadError(e)) => {
                            error!(channel=SWITCH_CHANNEL_ID_LOG; "Bug Read Error {e}");
                            return false;
                        }
                        _ => {}
                    }
                }
                _ => {}
            };
            true // continue 
        },
    )?;

    let res = {
        let channel = session.get_channel().ok_or(anyhow!("missing channel"))?;
        let key = CString::new(bug_name).unwrap();
        channel.set_private_with_key(&key, bug.clone())?;
        Ok(())
    };
    if let Err(err) = res {
        error!(channel=SWITCH_CHANNEL_ID_LOG; "Failure to record bug in channel: {err}");
        let _ = session.remove_media_bug(bug);
        return Err(err);
    }

    let response_handler = move |change: Body| {
        let _ = Event::new_custom_event(MOD_WSFORK_EVENT).and_then(|mut fs_event| {
            let data = WSForkEvent {
                session: session_id.clone(),
                body: change,
            };
            if let Some(session) = Session::locate(&session_id)
                && let Some(channel) = session.get_channel()
            {
                fs_event.set_channel_data(&channel);
            }
            let _ = fs_event.set_body(serde_json::to_string(&data).unwrap_or_default());
            fs_event.fire()
        });
    };

    RT.get().unwrap().spawn(async move {
        // TODO: Reconnection logic
        let req = rx.req.clone();
        let res = async move {
            let stream = TcpStream::connect(addr).await?;
            let executor = TokioExecutor::new();
            let (ws, _) = handshake::client(&executor, req, stream).await?;
            Ok::<_, anyhow::Error>(ws)
        }
        .await;

        match res {
            Err(e) => response_handler(wsfork_events::Body::Error {
                desc: format!("{:#}", e),
            }),
            Ok(ws) => {
                let _ = rx.run(ws, response_handler.clone()).await;
            }
        };
        drop(owned);
    });

    Ok(())
}
