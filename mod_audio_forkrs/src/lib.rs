use std::borrow::Cow;
use std::rc::Rc;
use clap::ArgAction;
use clap::value_parser;
use clap::{Command, Arg};
use freeswitch_sys::switch_abc_type_t_SWITCH_ABC_TYPE_CLOSE;
use freeswitch_sys::switch_abc_type_t_SWITCH_ABC_TYPE_INIT;
use freeswitch_sys::switch_abc_type_t_SWITCH_ABC_TYPE_READ;
use freeswitch_sys::switch_abc_type_t_SWITCH_ABC_TYPE_WRITE_REPLACE;
use freeswitch_sys::Session;

pub enum Error {
    InvalidArguments
}

#[repr(C)]
struct Private {
    foo: u8
}

pub enum ModSubcommand {
    Start { session:String, url : String },
    Stop { session: String },
}

fn fork() {
}

fn api_main(command:ModSubcommand) -> Result<(),Error>{
    match command {
        ModSubcommand::Start { session, url } => {
            let s = Session::locate(session).ok_or(Error::InvalidArguments)?;

            s.add_media_bug("".to_string(), "".to_string(), 0, |bug,abc_type| { 
                let s = bug.get_session();
                match abc_type {
                    switch_abc_type_t_SWITCH_ABC_TYPE_INIT => {},
                    switch_abc_type_t_SWITCH_ABC_TYPE_CLOSE => {},
                    switch_abc_type_t_SWITCH_ABC_TYPE_READ => {},
                    switch_abc_type_t_SWITCH_ABC_TYPE_WRITE_REPLACE => {}
                }
                return true
            });
        }
        _ => {}
    }
    Ok(())
}

fn parse_args(cmd_str:Cow<'_,str>) -> Result<ModSubcommand,Error> {
    let cmd = Command::new("prog")
        .subcommand(Command::new("start")
            .args(&[
                  Arg::new("uuid")
                        .value_parser(value_parser!(String))
                        .action(ArgAction::Set)
                        .required(true),
                  Arg::new("url")
                        .value_parser(value_parser!(String))
                        .action(ArgAction::Set)
                        .required(true)
            ])
        )
        .subcommand(Command::new("stop")
            .args(&[
                  Arg::new("uuid")
                        .value_parser(value_parser!(String))
                        .action(ArgAction::Set)
                        .required(true)
            ])
        );   

    let m = cmd.try_get_matches_from(cmd_str.split(' ')).map_err(|_| Error::InvalidArguments)?;
    match m.subcommand() {
        Some(("start", m)) => {
            let session = m.get_one::<String>("uuid").ok_or(Error::InvalidArguments)?;
            let url =  m.get_one::<String>("url").ok_or(Error::InvalidArguments)?;
            Ok(ModSubcommand::Start { url: url.to_owned(), session: session.to_owned() })
        }
        Some(("stop", m)) => {
            let session = m.get_one::<String>("uuid").ok_or(Error::InvalidArguments)?;
            Ok(ModSubcommand::Stop { session: session.to_owned() })
        }
        _ =>  Err(Error::InvalidArguments)
    }
}



