mod arg_parse;
mod audio_fork;

use anyhow::{Result, anyhow};
use fastwebsockets::handshake;
use hyper_util::rt::TokioExecutor;
use tokio::net::TcpStream;
use tokio::runtime::{Builder, Runtime};
use tokio::time::{sleep, timeout};

use wsfork_events::Body;
pub use wsfork_events::MOD_WSFORK_EVENT;
use wsfork_events::WSForkEvent;

use std::ffi::{CStr, CString};
use std::ops::Deref;
use std::sync::{Arc, Mutex, OnceLock, Weak, atomic::AtomicBool, atomic::Ordering};
use std::time::Duration;

use freeswitch_rs::log::{debug, error, info, warn};
use freeswitch_rs::*;

use crate::arg_parse::{Common, Subcommands, parse_args};
use crate::audio_fork::{WSForkerError, new_wsfork};

static RT: OnceLock<Runtime> = OnceLock::new();

const DEFAULT_BUG_KEY: &CStr = c"MOD_WSFORK_BUG_KEY";

struct PrivateSessionData {
    tx: audio_fork::WSForkSender,
    bug: Mutex<Option<MediaBugHandle>>,
    paused: AtomicBool,
}

#[switch_module_define(mod_wsfork)]
struct FSMod;

impl LoadableModule for FSMod {
    fn load(module: FSModuleInterface, _pool: FSModulePool) -> switch_status_t {
        info!(channel=SWITCH_CHANNEL_ID_LOG; "mod ws_fork loading");
        // TODO: make worker count configurable
        let Ok(runtime) = Builder::new_multi_thread()
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
    debug!(channel=SWITCH_CHANNEL_ID_LOG; "mod wsfork cmd {}", &cmd);

    let cmd = match parse_args(cmd) {
        Err(e) => {
            error!(channel=SWITCH_CHANNEL_ID_SESSION; "mod wsfork invalid usage:\n{}", &e);
            return switch_status_t::SWITCH_STATUS_SUCCESS;
        }
        Ok(cmd) => cmd,
    };

    let Common {
        session_id,
        bug_name,
    } = cmd.common_args();

    let Some(session) = Session::locate(session_id) else {
        error!(channel=SWITCH_CHANNEL_ID_SESSION; "Failed to find session {session_id}");
        return switch_status_t::SWITCH_STATUS_SUCCESS;
    };

    let bug_name = bug_name.clone().and_then(|s| CString::new(s).ok());
    let key = bug_name.as_ref().map_or(DEFAULT_BUG_KEY, |s| s.deref());
    let s = session_id.to_owned();

    let res = match &cmd {
        Subcommands::Start { url, .. } => api_start(&session, key, url.to_owned(), move |event| {
            response_handler(&s, event)
        }),
        other_cmds => {
            let data = unsafe {
                session
                    .get_channel()
                    .and_then(|c| c.get_private_raw_ptr(key))
                    .map(|ptr| Weak::from_raw(ptr as *const PrivateSessionData))
                    // make sure to 'downgrade' weak ptr to avoid drop!
                    .map(|p| (p.upgrade(), p.into_raw()))
                    .and_then(|t| t.0)
            };
            data.map(|data| match other_cmds {
                Subcommands::Stop { .. } => api_stop(&data),
                Subcommands::Pause { .. } => api_pause(&data, true),
                Subcommands::Resume { .. } => api_pause(&data, false),
                Subcommands::SendText { text, ..} => api_send_text(&data, text.to_owned()),
                _ => Ok(()),
            })
            .unwrap_or(Err(anyhow!("Failed to find fork for session {session_id}")))
        }
    };

    if let Err(e) = res {
        error!(channel=SWITCH_CHANNEL_ID_SESSION; "mod wsfork error: {}", &e);
        let _ = write!(stream, "-ERR, mod wsfork operation failed");
    } else {
        let _ = write!(stream, "+OK Success");
    }
    switch_status_t::SWITCH_STATUS_SUCCESS
}

fn api_pause(data: &PrivateSessionData, pause: bool) -> Result<()> {
    data.paused.store(pause, Ordering::Relaxed);
    Ok(())
}

fn api_stop(data: &PrivateSessionData) -> Result<()> {
    data.tx.cancel();
    Ok(())
}

fn api_send_text(data:&PrivateSessionData, msg:String) -> Result<()> {
    data.tx.send_message(msg.into_bytes())?;
    Ok(())
}

fn api_start(
    session: &Session,
    bug_name: &CStr,
    url: String,
    response_handler: impl Fn(Body) + Send + Sync + 'static,
) -> Result<()> {
    let url = url::Url::parse(&url)?;
    let addr = url.socket_addrs(|| None)?.pop().ok_or(anyhow!(""))?;

    let read_impl = unsafe {
        freeswitch_sys::switch_core_session_get_read_codec(session.as_ptr())
            .as_ref()
            .and_then(|c| c.implementation.as_ref())
            .ok_or(anyhow!(""))?
    };

    let frame_size = read_impl.decoded_bytes_per_packet;
    // Hardcode buffer length for now
    let buffer_duration = Duration::from_millis(100);
    let ms_per_packet = (read_impl.samples_per_packet / read_impl.samples_per_second) * 1000;
    let buffer_len = (buffer_duration.as_millis() as u32).div_ceil(ms_per_packet);

    let (tx, rx) = new_wsfork(
        url.clone(),
        frame_size as usize,
        buffer_len as usize,
        |_| {},
    )?;

    let mod_data = Arc::new(PrivateSessionData {
        tx,
        bug: Mutex::new(None),
        paused: AtomicBool::new(false),
    });

    let mut send_task = RT.get().unwrap().spawn(async move {
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
                let _ = rx.run(ws, &response_handler).await;
            }
        };
    });

    let bug = {
        let bug_name = bug_name.to_owned();
        let mod_data = mod_data.clone();

        session.add_media_bug(
        None,
        None,
        MediaBugFlags::SMBF_READ_STREAM,
        move |bug, abc_type| {
            // For error handling, if we return false from closure
            // FS will prune mal functioning bug ( ie remove it )
            let PrivateSessionData { tx, paused ,..} = &mod_data.deref();
            match abc_type {
                switch_abc_type_t::SWITCH_ABC_TYPE_CLOSE => {
                    // Wait for task to complete so we can ensure it
                    // doesn't hold any session resources
                    tx.cancel();
                    let res = RT.get().unwrap().block_on(async {
                        timeout(Duration::from_secs(5), &mut send_task).await
                    });
                    if res.is_err() {
                        warn!(channel=SWITCH_CHANNEL_ID_LOG; "Failed to cleanup sender task");
                    }
                    // clean up smart pointers in channel
                    unsafe {
                        if let Some(channel) = bug.get_session().get_channel() 
                            && let Some(ptr) = channel.get_private_raw_ptr(&bug_name)
                        {
                            let weak_ref = Weak::from_raw(ptr as *const PrivateSessionData);
                            let _ = channel.set_private_raw_ptr(&bug_name, std::ptr::null::<PrivateSessionData>());
                            RT.get().unwrap().spawn(async move {
                                // we need to ensure no-one else is reading the ptr ....
                                // current work around is to delay cleanup until way after 
                                // bug removal.
                                sleep(Duration::from_secs(30)).await;
                                drop(weak_ref);
                            });
                        } 
                        else {
                            warn!(channel=SWITCH_CHANNEL_ID_LOG; "Failed to cleanup Channel ptrs");
                        }
                            return false
                    };
                }

                switch_abc_type_t::SWITCH_ABC_TYPE_READ => {
                    if paused.load(std::sync::atomic::Ordering::Relaxed) {
                        return true
                    }
                    match tx.get_next_free_buffer() {
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
    )
    }?;

    // Save mod data in channel so future cmds can use
    mod_data.bug.lock().unwrap().replace(bug.clone());
    let data = Arc::downgrade(&mod_data);
    let res = unsafe {
        let channel = session.get_channel().ok_or(anyhow!("Missing Channel"))?;
        channel.set_private_raw_ptr(bug_name, Weak::into_raw(data))
    };
    if let Err(err) = res {
        error!(channel=SWITCH_CHANNEL_ID_LOG; "Failure to record bug in channel: {err}");
        let _ = session.remove_media_bug(bug);
        return Err(err.into());
    }

    Ok(())
}

fn response_handler(session_id: &str, change: Body) {
    let _ = Event::new_custom_event(MOD_WSFORK_EVENT).and_then(|mut fs_event| {
        let data = WSForkEvent {
            session: session_id.to_owned(),
            body: change,
        };
        if let Some(session) = Session::locate(session_id)
            && let Some(channel) = session.get_channel()
        {
            fs_event.set_channel_data(&channel);
        }
        let _ = fs_event.set_body(serde_json::to_string(&data).unwrap_or_default());
        fs_event.fire()
    });
}
