mod audio_fork;

pub use wsfork_events::MOD_WSFORK_EVENT_CONNECT;
pub use wsfork_events::MOD_WSFORK_EVENT_DISCONNECT;
pub use wsfork_events::MOD_WSFORK_EVENT_ERROR;
pub use wsfork_events::MOD_WSFORK_EVENT_SAMPLES_OVERRUN;
use wsfork_events::WSForkEvent;

use audio_fork::create_request;
use audio_fork::new_fork;

use anyhow::Result;
use anyhow::anyhow;
use clap::{Command, FromArgMatches as _, Parser, Subcommand as _};

use freeswitch_rs::Session;
use freeswitch_rs::switch_status_t;
use serde_json::json;
use std::io::ErrorKind;
use std::sync::LazyLock;
use std::sync::Mutex;
use tokio::net::TcpStream;
use tokio::runtime::Runtime;

use freeswitch_rs::log::{debug, error, info, warn};
use freeswitch_rs::*;

static RT: LazyLock<Runtime> =
    LazyLock::new(|| tokio::runtime::Builder::new_multi_thread().build().unwrap());

// Private data stored on channel for mod usage
struct Private {
    bug: Mutex<Option<MediaBugHandle>>,
}
impl Private {
    fn new(bug: MediaBugHandle) -> Self {
        Self {
            bug: Mutex::new(Some(bug)),
        }
    }
}

#[derive(Parser, Debug)]
enum Subcommands {
    Start {
        #[arg()]
        session: String,

        #[arg()]
        url: String,
    },
    Stop {
        #[arg()]
        session: String,
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
        module.add_api(api_main);

        if Event::reserve_subclass(MOD_WSFORK_EVENT_CONNECT).is_err()
            || Event::reserve_subclass(MOD_WSFORK_EVENT_DISCONNECT).is_err()
            || Event::reserve_subclass(MOD_WSFORK_EVENT_ERROR).is_err()
        {
            error!(channel=SWITCH_CHANNEL_ID_LOG; "Failure to register custom events");
            return switch_status_t::SWITCH_STATUS_TERM;
        }

        switch_status_t::SWITCH_STATUS_SUCCESS
    }

    fn shutdown() -> switch_status_t {
        info!(channel=SWITCH_CHANNEL_ID_LOG; "mod ws_fork shutdown");

        let _ = Event::free_subclass(MOD_WSFORK_EVENT_CONNECT);
        let _ = Event::free_subclass(MOD_WSFORK_EVENT_DISCONNECT);
        let _ = Event::free_subclass(MOD_WSFORK_EVENT_ERROR);

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
                Subcommands::Start { session, url } => api_start(session, url),
                Subcommands::Stop { session } => api_stop(session),
            };
            if let Err(e) = res {
                error!(channel=SWITCH_CHANNEL_ID_SESSION; "mod audiofork error: {}", &e);
            }
        }
    }
    switch_status_t::SWITCH_STATUS_SUCCESS
}

fn api_stop(uuid: String) -> Result<()> {
    let s = Session::locate(&uuid).ok_or(anyhow!("Session Not Found: {}", uuid))?;
    let channel = s.get_channel().unwrap();

    let Some(data) = channel.get_private::<Private>() else {
        return Err(anyhow!("Private Session data not Found: {}", uuid));
    };

    // removing bug consumes the struct
    if let Ok(Some(bug)) = data.bug.lock().map(|mut b| b.take()) {
        s.remove_media_bug(bug)?;
    }
    Ok(())
}

fn api_start(uuid: String, url: String) -> Result<()> {
    debug!(channel=SWITCH_CHANNEL_ID_SESSION; "mod audiofork start uuid:{}",uuid);

    let req = create_request(url.clone(), |req| {})?;
    let (mut handle, fork) = new_fork(req, SWITCH_RECOMMENDED_BUFFER_SIZE * 256);

    let session = Session::locate(&uuid).ok_or(anyhow!("Session Not Found: {}", uuid))?;
    let bug = session.add_media_bug("".to_string(), "".to_string(), 0, move |bug, abc_type| {
        // For error handling, if we return false from closure
        // FS will prune mal functioning bug ( ie remove it )
        let should_continue = true;
        match abc_type {
            switch_abc_type_t::SWITCH_ABC_TYPE_INIT => {}
            switch_abc_type_t::SWITCH_ABC_TYPE_CLOSE => handle.close(),
            switch_abc_type_t::SWITCH_ABC_TYPE_READ => {
                let n = 0; // TODO: exact size of packet
                match handle.copy_samples(bug, n) {
                    Ok(_) => {}
                    Err(e) if e.kind() == ErrorKind::StorageFull => {
                        debug!(channel=SWITCH_CHANNEL_ID_LOG; "Packets Dropped")
                    }
                    Err(e) => {
                        error!(channel=SWITCH_CHANNEL_ID_LOG; "Error Reading frame for bug: {}", e);
                        return false;
                    }
                };
            }
            _ => {}
        };
        should_continue
    })?;

    // run forker in background
    let id = uuid.clone();
    let response_handler = async move {
        let stream = TcpStream::connect(url).await.map_err(|e| anyhow!(e))?;
        fork.run(stream).await?;
        Ok::<(), anyhow::Error>(())
    };

    RT.spawn(async move {
        match response_handler.await {
            Ok(()) => {}
            Err(err) => {
                let event = WSForkEvent {
                    session: uuid,
                    body: wsfork_events::Body::Error {},
                };
                let _ = Event::new_custom_event(event.tag()).and_then(|mut e| {
                    if let Some(session) = Session::locate(&id)
                        && let Some(channel) = session.get_channel()
                    {
                        e.set_channel_data(channel);
                    }
                    let _ = e.set_body(serde_json::to_string(&event).unwrap_or_default());
                    e.fire()
                });
            }
        }
    });

    // Set private data into channel for later retrieval
    let channel = session.get_channel().unwrap();
    unsafe {
        channel.set_private(Box::new(Private::new(bug)))?;
    }
    let _ = channel.add_state_handler(&STATE_HANDLERS);

    Ok(())
}

// Rust data isn't allocated in the session memory pool currently
// so there is a need to register a manual cleanup
#[switch_state_handler]
pub fn cleanup(s: &Session) -> switch_status_t {
    unsafe {
        // take back the box and let drop run
        let _ = s.get_channel().map(|c| c.remove_private::<Private>());
    }
    switch_status_t::SWITCH_STATUS_SUCCESS
}

const STATE_HANDLERS: StateHandlerTable = StateHandlerTable {
    on_destroy: Some(cleanup),
    ..DEFAULT_STATE_HANDLER_TABLE
};
