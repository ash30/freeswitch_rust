use crate::arg_parse::{AudioMix, Endpoint};
use crate::audio_fork::{
    WSForkReceiver, WSForkSender, WSForkerError, new_wsfork, run_io_loop, 
};
use anyhow::{Result, anyhow};
use freeswitch_rs::Frame;
use freeswitch_rs::core::{MediaBugFlags, MediaBugHandle, Session, SessionExt};
use freeswitch_rs::log::{debug, error, warn};
use freeswitch_rs::prelude::*;
use freeswitch_rs::types::switch_abc_type_t;
use freeswitch_sys::switch_codec_implementation;
use std::ffi::CStr;
use std::ops::Deref;
use std::sync::{Arc, Mutex, Weak, atomic::AtomicBool, atomic::Ordering};
use std::time::Duration;
use tokio::runtime;
use tokio::time::{sleep, timeout};
use wsfork_events::Body;

pub(crate) struct PrivateSessionData {
    tx: WSForkSender,
    bug: Mutex<Option<MediaBugHandle>>,
    paused: AtomicBool,
}

impl PrivateSessionData {
    pub(crate) fn get(s: &Session, fork_name: &CStr) -> Option<impl Deref<Target = Self>> {
        unsafe {
            s.get_channel()
                .and_then(|c| c.get_private_raw_ptr(fork_name))
                .map(|ptr| Weak::from_raw(ptr as *const PrivateSessionData))
                .map(|p| (p.clone(), p.into_raw()))
                .and_then(|(p, _)| p.upgrade())
        }
    }

    pub(crate) fn init(
        read_impl: &switch_codec_implementation,
        audio_mix: AudioMix,
    ) -> Result<(PrivateSessionData, WSForkReceiver)> {
        let frame_size = match audio_mix {
            AudioMix::Stereo => read_impl.decoded_bytes_per_packet * 2,
            _ => read_impl.decoded_bytes_per_packet,
        };
        // Hardcode buffer length for now
        let buffer_duration = Duration::from_millis(100);
        let buffer_len = read_impl
            .microseconds_per_packet
            .try_into()
            .map(|n: u128| buffer_duration.as_micros().div_ceil(n.max(1)))
            .map(|n| n.clamp(1, 5))
            .unwrap_or(3);

        let (tx, rx) = new_wsfork(frame_size as usize, buffer_len as usize)?;

        Ok((
            PrivateSessionData {
                tx,
                bug: Mutex::new(None),
                paused: AtomicBool::new(false),
            },
            rx,
        ))
    }

    pub fn pause(&self, state: bool) -> Result<()> {
        self.paused.store(state, Ordering::Relaxed);
        Ok(())
    }
    pub(crate) fn stop(&self) -> Result<()> {
        self.tx.cancel();
        Ok(())
    }

    pub(crate) fn send_text(&self, msg: String) -> Result<()> {
        self.tx.send_message(msg.into_bytes())?;
        Ok(())
    }
}

pub(crate) fn api_start(
    session: &Session,
    fork_name: &CStr,
    endpoint: &Endpoint,
    audio_mix: AudioMix,
    start_paused: bool,
    response_handler: impl Fn(Body) + Send + Sync + 'static + Clone,
    runtime: &'static runtime::Runtime,
) -> Result<()> {
    debug!(logger:session_log!(session), "Getting Read Impl");
    let read_impl = unsafe {
        freeswitch_sys::switch_core_session_get_read_codec(session.as_ptr())
            .as_ref()
            .and_then(|c| c.implementation.as_ref())
            .ok_or(anyhow!(""))?
    };

    debug!(logger:session_log!(session), "Init");
    let (data, rx) = PrivateSessionData::init(read_impl, audio_mix)?;
    let _ = data.pause(start_paused);
    let mod_data = Arc::new(data);

    let addr = endpoint.addr()?;
    let req = endpoint.to_request()?;
    let mut send_task = runtime.spawn(run_io_loop(addr, req, rx, response_handler));

    debug!(logger:session_log!(session), "Attaching Bug");
    let flags = match audio_mix {
        AudioMix::Mono => MediaBugFlags::SMBF_READ_STREAM,
        AudioMix::Mixed => MediaBugFlags::SMBF_READ_STREAM | MediaBugFlags::SMBF_WRITE_STREAM,
        AudioMix::Stereo => {
            MediaBugFlags::SMBF_READ_STREAM
                | MediaBugFlags::SMBF_WRITE_STREAM
                | MediaBugFlags::SMBF_STEREO
        }
    };
    let bug = {
        let fork_name = fork_name.to_owned();
        let mod_data = mod_data.clone();
        session.add_media_bug(None, None, flags, move |bug, abc_type| {
            let PrivateSessionData { tx, paused, .. } = &mod_data.deref();
            match abc_type {
                switch_abc_type_t::SWITCH_ABC_TYPE_CLOSE => {
                    // Wait for task to complete so we can ensure it
                    // doesn't hold any session resources
                    tx.cancel();
                    if runtime
                        .block_on(timeout(Duration::from_secs(5), &mut send_task)).is_err() 
                    {
                        warn!(logger:session_log!(&bug.get_session()), "Failed to cleanup sender task");
                        send_task.abort();
                    }
                    // clean up smart pointers in channel
                    unsafe {
                        if let Some(channel) = bug.get_session().get_channel()
                            && let Some(ptr) = channel.get_private_raw_ptr(&fork_name)
                        {
                            let weak_ref = Weak::from_raw(ptr as *const PrivateSessionData);
                            let _ = channel.set_private_raw_ptr(
                                &fork_name,
                                std::ptr::null::<PrivateSessionData>(),
                            );
                            runtime.spawn(async move {
                                // we need to ensure no-one else is reading the ptr ....
                                // current work around is to delay cleanup until way after
                                // bug removal.
                                sleep(Duration::from_secs(5)).await;
                                drop(weak_ref);
                            });
                        } else {
                            warn!(logger:session_log!(&bug.get_session()), "Failed to cleanup Channel ptrs");
                        }
                        return false;
                    };
                }

                switch_abc_type_t::SWITCH_ABC_TYPE_READ => {
                    if paused.load(std::sync::atomic::Ordering::Relaxed) {
                        return true;
                    }
                    match tx.get_next_free_buffer() {
                        Err(WSForkerError::Full) => {
                            warn!(logger:session_log!(&bug.get_session()), "Buffer full + Packets dropped");
                        }
                        Err(WSForkerError::Closed) => {
                            // WS has closed, stop bug
                            debug!(logger:session_log!(&bug.get_session()), "WS Closed, Pruning bug");
                            return false;
                        }
                        Ok(mut b) => {
                            let mut f = Frame::new(&mut b);
                            if let Err(e) = bug.read_frame(&mut f) {
                                error!(logger:session_log!(&bug.get_session()), "Error Reading Frame {e}");
                                return false;
                            }
                        }
                    };
                }
                _ => {}
            };
            true // continue 
        })
    }?;

    debug!(logger:session_log!(session), "saving bug in channel");
    mod_data.bug.lock().unwrap().replace(bug.clone());

    let data = Arc::downgrade(&mod_data).into_raw();
    match session
        .get_channel()
        .map(|c| unsafe { c.set_private_raw_ptr(fork_name, data) })
    {
        Some(Ok(_)) => Ok(()),
        e => {
            let _ = session.remove_media_bug(bug);
            match e {
                None => Err(anyhow!("Failed to find Channel")),
                Some(res) => Ok(res?),
            }
        }
    }
}
