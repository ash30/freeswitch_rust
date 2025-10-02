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
use std::ops::Deref;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::net::TcpStream;

use freeswitch_rs::log::{debug, error, info, warn};
use freeswitch_rs::*;

use crate::audio_fork::WSForkerError;
use crate::audio_fork::new_wsfork;

static RT: OnceLock<Runtime> = OnceLock::new();

const DEFAULT_BUG_KEY: &CStr = c"MOD_WSFORK_BUG_KEY";

#[derive(Parser, Debug)]
enum Subcommands {
    Start {
        #[arg()]
        session: String,
        #[arg()]
        url: String,
        bug_name: Option<String>,
    },
    Stop {
        #[arg()]
        session: String,
        bug_name: Option<String>,
    },
}

fn parse_args(cmd_str: &str) -> Result<Subcommands> {
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

#[switch_module_define(mod_wsfork)]
struct FSMod;

impl LoadableModule for FSMod {
    fn load(module: FSModuleInterface, _pool: FSModulePool) -> switch_status_t {
        info!(channel=SWITCH_CHANNEL_ID_LOG; "mod ws_fork loading");
        // TODO: make worker count configurable
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

#[switch_api_define(name = "wsfork", desc = "fork audio frames over websocket")]
fn api_main(cmd: &str, _session: Option<&Session>, mut stream: StreamHandle) -> switch_status_t {
    info!(channel=SWITCH_CHANNEL_ID_LOG; "mod wsfork cmd {}", &cmd);
    match parse_args(cmd) {
        Err(e) => {
            let _ = write!(stream, "-ERR, mod wsfork invalid usage\n{}", e);
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
                error!(channel=SWITCH_CHANNEL_ID_SESSION; "mod wsfork error: {}", &e);
            }
        }
    }
    switch_status_t::SWITCH_STATUS_SUCCESS
}

fn api_stop(session_id: String, bug_name: Option<String>) -> Result<()> {
    let bug_name = bug_name.and_then(|s| CString::new(s).ok());
    let key = bug_name.as_ref().map_or(DEFAULT_BUG_KEY, |s| s.deref());
    let (session, bug) = Session::locate(&session_id)
        .and_then(|s| unsafe {
            s.get_channel()
                .and_then(|c| c.get_private_with_key::<MediaBugHandle>(key).cloned())
                .map(|b| (s, b))
        })
        .ok_or(anyhow!("Unable to find bug for session {session_id}"))?;
    session.remove_media_bug(bug)?;
    Ok(())
}

fn api_start(session_id: String, url: String, bug_name: Option<String>) -> Result<()> {
    debug!(channel=SWITCH_CHANNEL_ID_SESSION; "mod wsfork start uuid:{}",session_id);
    let session =
        Session::locate(&session_id).ok_or(anyhow!("Session Not Found: {}", session_id))?;

    let url = url::Url::parse(&url)?;
    let mut addrs = url.socket_addrs(|| None)?;
    let addr = addrs.pop().ok_or(anyhow!(""))?;

    let frame_size = 0;
    let buf_duration = Duration::from_millis(20);

    let (tx, rx) = new_wsfork(url.clone(), frame_size, buf_duration, |_| {})?;
    let owned = Arc::new(tx);
    let weak_ref = Arc::downgrade(&owned);

    let bug = session.add_media_bug(
        None,
        None,
        MediaBugFlags::SMBF_READ_STREAM,
        move |bug, abc_type| {
            // For error handling, if we return false from closure
            // FS will prune mal functioning bug ( ie remove it )
            let Some(handle) = weak_ref.upgrade() else {
                return false;
            };
            match abc_type {
                switch_abc_type_t::SWITCH_ABC_TYPE_CLOSE => {
                    handle.cancel();
                }
                switch_abc_type_t::SWITCH_ABC_TYPE_READ => {
                    match handle.get_next_free_buffer() {
                        Err(WSForkerError::Full) => {
                            warn!(channel=SWITCH_CHANNEL_ID_LOG; "Buffer full + Packets dropped")
                        }
                        Err(WSForkerError::Closed) => {
                            // WS has closed, stop bug
                            debug!(channel=SWITCH_CHANNEL_ID_LOG; "WS Closed, Pruning bug");
                            return false;
                        }
                        Ok(mut b) => {
                            let mut f = Frame::new(&mut b);
                            info!(channel=SWITCH_CHANNEL_ID_LOG; "data_size {}", f.data().len());
                            if let Err(e) = bug.read_frame(&mut f) {
                                error!(channel=SWITCH_CHANNEL_ID_LOG; "Error Reading Frame {e}");
                                return false;
                            }
                        }
                    };
                }
                _ => {}
            };
            true // continue 
        },
    )?;

    let res = {
        let channel = session.get_channel().ok_or(anyhow!("missing channel"))?;
        let bug_name = bug_name.and_then(|s| CString::new(s).ok());
        let key = bug_name.as_ref().map_or(DEFAULT_BUG_KEY, |s| s.deref());
        channel.set_private_with_key(key, bug.clone())?;
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
