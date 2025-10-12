mod arg_parse;
mod audio_fork;
mod mod_wsfork_api;

use crate::arg_parse::{Common, Subcommands, parse_args};
use anyhow::anyhow;
use freeswitch_rs::prelude::*;
use freeswitch_rs::{
    core::{Session, SessionExt},
    event::Event,
    log::*,
};
use std::ffi::CStr;
use std::ops::Deref;
use std::sync::OnceLock;
use tokio::runtime::{Builder, Runtime};
use wsfork_events::{Body, WSForkEvent};

pub use wsfork_events::MOD_WSFORK_EVENT;

static RT: OnceLock<Runtime> = OnceLock::new();

#[switch_module_define(mod_wsfork)]
struct FSMod;

impl LoadableModule for FSMod {
    fn load(module: FSModuleInterface, _pool: FSModulePool) -> switch_status_t {
        info!("mod ws_fork loading");
        // TODO: make worker count configurable
        let Ok(runtime) = Builder::new_multi_thread()
            .enable_all()
            .worker_threads(5)
            .build()
        else {
            return switch_status_t::SWITCH_STATUS_FALSE;
        };
        let _ = RT.set(runtime);

        module.add_api(api_main);

        if Event::reserve_subclass(MOD_WSFORK_EVENT).is_err() {
            error!("Failure to register custom events");
            return switch_status_t::SWITCH_STATUS_FALSE;
        }

        switch_status_t::SWITCH_STATUS_SUCCESS
    }

    fn shutdown() -> switch_status_t {
        info!("mod ws_fork shutdown");
        let _ = Event::free_subclass(MOD_WSFORK_EVENT);
        switch_status_t::SWITCH_STATUS_SUCCESS
    }
}

#[switch_api_define(name = "wsfork", desc = "fork audio frames over websocket")]
fn api_main(cmd: &str, _session: Option<&Session>, mut stream: StreamHandle) -> switch_status_t {
    debug!("mod wsfork cmd {}", &cmd);

    let cmd = match parse_args(cmd) {
        Err(e) => {
            error!("mod wsfork invalid usage:\n{}", &e);
            return switch_status_t::SWITCH_STATUS_SUCCESS;
        }
        Ok(cmd) => cmd,
    };

    let Common {
        session_id,
        fork_id,
    } = cmd.common_args();

    let Some(session) = Session::locate(session_id) else {
        error!(
            "Failed to find session {}",
            session_id.to_owned().into_string().unwrap_or_default()
        );
        return switch_status_t::SWITCH_STATUS_SUCCESS;
    };

    let res = match &cmd {
        Subcommands::Start {
            endpoint,
            start_paused,
            mix,
            ..
        } => {
            let s = session_id.to_owned();
            let runtime = RT.get().expect("async runtime has been initialised");
            mod_wsfork_api::api_start(
                &session,
                fork_id,
                endpoint,
                *mix,
                start_paused.unwrap_or(false),
                move |event| response_handler(&s, event),
                runtime,
            )
        }
        other_cmds => mod_wsfork_api::PrivateSessionData::get(&session, fork_id)
            .as_deref()
            .map(|data| match other_cmds {
                Subcommands::Stop { .. } => data.stop(),
                Subcommands::Pause { .. } => data.pause(true),
                Subcommands::Resume { .. } => data.pause(false),
                Subcommands::SendText { text, .. } => data.send_text(text.to_owned()),
                _ => Ok(()),
            })
            .unwrap_or(Err(anyhow!(
                "Failed to find fork for session {}",
                session_id.to_owned().into_string().unwrap_or_default()
            ))),
    };

    if let Err(e) = res {
        error!(logger:session_log!(session.deref()), "mod wsfork error: {}", &e);
        let _ = write!(stream, "-ERR, mod wsfork operation failed");
    } else {
        let _ = write!(stream, "+OK Success");
    }

    switch_status_t::SWITCH_STATUS_SUCCESS
}

fn response_handler(session_id: &CStr, change: Body) {
    let _ = Event::new_custom_event(MOD_WSFORK_EVENT).and_then(|mut fs_event| {
        let data = WSForkEvent {
            session: session_id.to_owned().into_string().unwrap_or_default(),
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
