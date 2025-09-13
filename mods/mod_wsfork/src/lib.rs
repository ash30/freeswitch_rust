mod audio_fork;

use audio_fork::create_request;
use audio_fork::new_fork;

use anyhow::anyhow;
use anyhow::Result;
use clap::{Command, FromArgMatches as _, Parser, Subcommand as _};

use std::ffi::CStr;
use std::sync::LazyLock;
use tokio::net::TcpStream;

use freeswitch_rs::log::{debug, error, info};
use freeswitch_rs::*;
use tokio::runtime::Runtime;

const EVENT_CONNECT: &CStr = c"CONNECT";
const EVENT_DISCONNECT: &CStr = c"DISCONNECT";
const EVENT_ERROR: &CStr = c"ERROR";

static RT: LazyLock<Runtime> =
    LazyLock::new(|| tokio::runtime::Builder::new_multi_thread().build().unwrap());

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

        if Event::reserve_subclass(EVENT_CONNECT).is_err()
            || Event::reserve_subclass(EVENT_DISCONNECT).is_err()
            || Event::reserve_subclass(EVENT_ERROR).is_err()
        {
            error!(channel=SWITCH_CHANNEL_ID_LOG; "Failure to register custom events");
            return switch_status_t::SWITCH_STATUS_TERM;
        }

        switch_status_t::SWITCH_STATUS_SUCCESS
    }

    fn shutdown() -> switch_status_t {
        info!(channel=SWITCH_CHANNEL_ID_LOG; "mod ws_fork shutdown");

        let _ = Event::free_subclass(EVENT_CONNECT);
        let _ = Event::free_subclass(EVENT_DISCONNECT);
        let _ = Event::free_subclass(EVENT_ERROR);

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
    let mut c = s.get_channel().unwrap();
    let Some(bug) = c.get_private::<MediaBug>() else {
        return Err(anyhow!("Bug Not Found: {}", uuid));
    };
    let _ = s.remove_media_bug(bug);
    Ok(())
}

fn api_start(uuid: String, url: String) -> Result<()> {
    debug!(channel=SWITCH_CHANNEL_ID_SESSION; "mod audiofork start uuid:{}",uuid);

    let req = create_request(url.clone(), |req| {})?;
    let (mut handle, fork) = new_fork(req, SWITCH_RECOMMENDED_BUFFER_SIZE);

    let s = Session::locate(&uuid).ok_or(anyhow!("Session Not Found: {}", uuid))?;
    let bug = s.add_media_bug(
        "".to_string(),
        "".to_string(),
        0,
        move |mut bug, abc_type| {
            // For error handling, if we return false from closure
            // FS will prune mal functioning bug ( ie remove it )
            let mut should_continue = true;
            match abc_type {
                switch_abc_type_t::SWITCH_ABC_TYPE_INIT => {}
                switch_abc_type_t::SWITCH_ABC_TYPE_CLOSE => handle.close(),
                switch_abc_type_t::SWITCH_ABC_TYPE_READ => {
                    should_continue = loop {
                        // read data from bug until FS returns non success
                        match handle.write(&mut bug) {
                            None => {} // buffer was full ... flush
                            Some(Ok(n)) => {
                                if n == 0 {
                                    break true; // nothing left
                                }
                                continue;
                            }
                            Some(Err(e)) => {
                                // AN IO Error .... fail fast?
                                // notify via event
                                break false;
                            }
                        }
                    };
                }
                _ => {}
            };
            should_continue
        },
    )?;

    // run forker in background
    RT.spawn(async move {
        // TODO: Errors
        let Ok(stream) = TcpStream::connect(url).await else {
            // TOD
            let _ = Event::new_custom_event(EVENT_ERROR).and_then(|e| e.fire());
            return;
        };

        if let Err(e) = fork.run(stream).await {
            // send event
        }
    });

    let mut channel = s.get_channel().unwrap();
    channel.set_private(bug)?;

    Ok(())
}
