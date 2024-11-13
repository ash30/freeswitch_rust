use anyhow::Ok;
use clap::ArgAction;
use clap::value_parser;
use clap::{Command, Arg};
use std::borrow::Borrow;
use std::borrow::BorrowMut;
use ringbuf::{traits::*, LocalRb};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tungstenite::accept;
use anyhow::Result;
use anyhow::anyhow;
use clap::{Command, FromArgMatches as _, Parser, Subcommand as _};

mod ws;

use freeswitch_rs::log::{info, debug};
use freeswitch_rs::SWITCH_CHANNEL_ID_LOG;
use freeswitch_rs::SWITCH_CHANNEL_ID_SESSION;
use freeswitch_rs::*;

pub enum Error {
    InvalidArguments,
}

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
    let mut cmd = Command::new("argparse");
    cmd = Subcommands::augment_subcommands(cmd);
    
    let matches = cmd.get_matches_from(cmd_str.split(' '));
    Subcommands::from_arg_matches(&matches).map_err(|_| Error::InvalidArguments)
}

#[repr(C)]
struct Private {
    foo: u8
}

#[switch_module_define(mod_audiofork)]
struct FSMod;

impl LoadableModule for FSMod {
    fn load(module: FSModuleInterface, pool: FSModulePool) -> switch_status_t {
        info!(channel = SWITCH_CHANNEL_ID_LOG; "mod ws_fork loading");
        module.add_api(api_main);
        switch_status_t::SWITCH_STATUS_SUCCESS
    }
}

#[switch_api_define("ws_fork")]
fn api_main(cmd:&str, _session:Option<Session>, mut stream:StreamHandle) -> switch_status_t {
    debug!(channel = SWITCH_CHANNEL_ID_SESSION; "mod audiofork cmd {}", &cmd);
    match parse_args(cmd) {
        Err(_) => {
            write!(stream, "ERR: mod audiofork invalid usage");
        },
        Ok(cmd) => {
            match cmd {
                Subcommands::Start { session, ip, port} => api_start(session, ip, port),
                Subcommands::Stop { session } => api_stop(session)
            }
        }
    }
    switch_status_t::SWITCH_STATUS_SUCCESS
}

fn api_stop(uuid:String) -> Result<()>{
    let s = Session::locate(&uuid).ok_or(anyhow!("Session Not Found: {}", uuid))?;

    Ok(())
}

fn api_start(uuid:String, ip:String, port:u16) -> Result<()> {
    debug!(channel = SWITCH_CHANNEL_ID_SESSION; "mod audiofork start uuid:{}",uuid);

    // We can locate session and have RAII guard unlock for us when scope finishes
    let s = Session::locate(&uuid).ok_or(anyhow!("Session Not Found: {}", uuid))?;
    let mut buf = LocalRb::new(SWITCH_RECOMMENDED_BUFFER_SIZE);

    // TODO: get address from args / channel 
    let ip_addr = IpAddr::V4(ip.parse());
    let addr = SocketAddr::new(ip_addr, port);
    let stream = std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(1))?;
    let mut ws = accept(stream).unwrap();

    let bug = s.add_media_bug("".to_string(), "".to_string(), 0, move |mut bug, abc_type| { 
        // For error handling, if we return false from closure
        // FS will prune mal functioning bug ( ie remove it ) 
        match abc_type {
            switch_abc_type_t::SWITCH_ABC_TYPE_INIT => {}
            switch_abc_type_t::SWITCH_ABC_TYPE_CLOSE => {
                ws.close(None)
            }
            switch_abc_type_t::SWITCH_ABC_TYPE_READ => {
                let mut b = FrameBuffer::default();
                loop {
                    // read data from bug until FS returns non success 
                    let Ok(n) = bug.read_frame(b.borrow_mut()) else { break };

                    // in theory, `n` should be `decoded_bytes_per_packet` size
                    if n == 0 { break }
                    buf.push_slice_overwrite(b.borrow());

                    // TODO: REALLY! want to remove this allocation ...
                    let v:Vec<u8> = buf.pop_iter().collect();
                    match ws.send(tungstenite::Message::binary(v)) {
                        Err(e @ tungstenite::Error::ConnectionClosed) => {
                            info!(channel == SWITCH_CHANNEL_ID_SESSION; "ws connection closing for session fork: {}", uuid);
                            // TODO Send event
                            return false 
                        },
                        Err(e) => {
                            // All other errors are considered fatal
                            return false 
                        },
                        Err( err @ std::io::Error) => if let std::io::ErrorKind::WouldBlock = err.kind() { 
                            // Shouldn't get here...
                        }
                        Err(tungstenite::Error::WriteBufferFull(m)) => {
                            // drop packets...
                        },
                        Ok(_) => {
                            // continue 
                        }
                    }   
                }
            }
            _ => {}
        };
        return true
    });
    Ok(())
}


// ========



