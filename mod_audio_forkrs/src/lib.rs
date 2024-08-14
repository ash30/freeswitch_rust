use std::borrow::Cow;
use clap::ArgAction;
use clap::value_parser;
use clap::{Command, Arg};

pub enum Error {
    InvalidArguments
}

pub enum ModSubcommand {
    Start { url : String },
    Stop { session: String },
}

fn parse_args(cmd_str:Cow<'_,str>) -> Result<ModSubcommand,Error> {
    let cmd = Command::new("prog")
        .subcommand(Command::new("start")
            .args(&[
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
            let url =  m.get_one::<String>("url").ok_or(Error::InvalidArguments)?;
            Ok(ModSubcommand::Start { url: url.to_owned() })
        }
        Some(("stop", m)) => {
            let session = m.get_one::<String>("uuid").ok_or(Error::InvalidArguments)?;
            Ok(ModSubcommand::Stop { session: session.to_owned() })
        }
        _ =>  Err(Error::InvalidArguments)
    }
}



