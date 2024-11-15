use std::borrow::Borrow;
use std::borrow::BorrowMut;
use anyhow::Result;
use anyhow::anyhow;
use clap::{Command, FromArgMatches as _, Parser, Subcommand as _};

use freeswitch_rs::log::{info, debug};
use freeswitch_rs::*;
use ws::WSForker;

const BUG_CHANNEL_KEY: &core::ffi::CStr = c"_WS_FORK_BUG";

#[derive(Parser, Debug)]
enum Subcommands {
    Start {
        #[arg()]
        session: String,

        #[arg()]
        ip: String,

        #[arg()]
        port: u16,
    },
    Stop {
        #[arg()]
        session: String,
    },
}

fn parse_args(cmd_str:&str) -> Result<Subcommands> {
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
        switch_status_t::SWITCH_STATUS_SUCCESS
    }
}

#[switch_api_define("ws_fork")]
fn api_main(cmd:&str, _session:Option<Session>, mut stream:StreamHandle) -> switch_status_t {
    debug!(channel=SWITCH_CHANNEL_ID_SESSION; "mod audiofork cmd {}", &cmd);
    match parse_args(cmd) {
        Err(_) => {
            let _ = write!(stream, "ERR: mod audiofork invalid usage");
        },
        Ok(cmd) => {
            let _ = match cmd {
                Subcommands::Start { session, ip, port} => api_start(session, ip, port),
                Subcommands::Stop { session } => api_stop(session)
            };
        }
    }
    switch_status_t::SWITCH_STATUS_SUCCESS
}

fn api_stop(uuid:String) -> Result<()>{
    let s = Session::locate(&uuid).ok_or(anyhow!("Session Not Found: {}", uuid))?;
    let mut c = s.get_channel();
    // TODO: Can we make this safer?
    let Some(bug) = c.get_private_unsafe(BUG_CHANNEL_KEY) else {
        return Err(anyhow!("Bug Not Found: {}", uuid))
    };
    s.remove_media_bug(bug);
    Ok(())
}

fn api_start(uuid:String, ip:String, port:u16) -> Result<()> {
    debug!(channel=SWITCH_CHANNEL_ID_SESSION; "mod audiofork start uuid:{}",uuid);

    // We can locate session and have RAII guard unlock for us when scope finishes
    let s = Session::locate(&uuid).ok_or(anyhow!("Session Not Found: {}", uuid))?;
    let mut forker = WSForker::new(SWITCH_RECOMMENDED_BUFFER_SIZE);
    forker.connect(ip, port)?;

    let bug = s.add_media_bug("".to_string(), "".to_string(), 0, move |mut bug, abc_type| { 
        // For error handling, if we return false from closure
        // FS will prune mal functioning bug ( ie remove it ) 
        let mut should_continue = true;
        match abc_type {
            switch_abc_type_t::SWITCH_ABC_TYPE_INIT => {}
            switch_abc_type_t::SWITCH_ABC_TYPE_CLOSE => forker.close(),
            switch_abc_type_t::SWITCH_ABC_TYPE_READ => {
                let mut b = FrameBuffer::default();
                should_continue = loop {
                    // read data from bug until FS returns non success 
                    let Ok(n) = bug.read_frame(b.borrow_mut()) else { break true };
                    // in theory, `n` should be `decoded_bytes_per_packet` size
                    if n == 0 { break true }
                    if !forker.fork(b.borrow()) { break false } else { continue };
                };
            }
            _ => {}
        };
        should_continue
    });

    // TODO: handle bug result
    let mut channel = s.get_channel();
    channel.set_private(BUG_CHANNEL_KEY, bug.unwrap());

    Ok(())
}




