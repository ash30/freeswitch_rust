use std::borrow::Cow;
use clap::ArgAction;
use clap::value_parser;
use clap::{Command, Arg};
use freeswitch_sys::switch_abc_type_t;
use freeswitch_rs::Session;
use tokio::runtime::Runtime;
use std::sync::OnceLock;

pub enum Error {
    InvalidArguments,
    RuntimeInit
}

#[repr(C)]
struct Private {
    foo: u8
}

pub enum ModSubcommand {
    Start { session:String, url : String },
    Stop { session: String },
}

static RT: OnceLock<Runtime> = OnceLock::new();

// Create async runtime at module load 
fn load() {
    let rt = RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .build()
            .unwrap()
    });
}

fn api_main(command:ModSubcommand) -> Result<(),Error>{
    match command {
        ModSubcommand::Start { session, url } => {
            // We can locate session and have RAII guard unlock for us when scope finishes
            let s = Session::locate(&session).ok_or(Error::InvalidArguments)?;

            // We can allocate arbitrary mod data types to session memory pool
            // we ensure safety by only allowing session objects to deref back the handle's ptr
            let data = Private { foo : 1};
            let handle = s.insert(data).map_err(|_|Error::InvalidArguments)?;

            let (tx,rx) = tokio::sync::mpsc::unbounded_channel();
            let rt = RT.get().ok_or(Error::RuntimeInit)?;
            rt.spawn(async move {
                // Setup WS 
                loop {
                    let Some(frame) = rx.recv().await else { break };
                    // send to ws 
                }
            });

            // Closures with captured state simplifies user data retrieval
            // Bug Callback will run on session thread/different thread, so any closure vars must be Send (assumedly)
            let bug = s.add_media_bug("".to_string(), "".to_string(), 0, move |bug, abc_type| { 
                let mut s = bug.get_session();

                // Session data can only be retrieved from Sesssion Object which implies you have
                // lock (again assumedly...)
                let mut d = s.get_mut(&handle).unwrap();
                d.as_mut().foo = 2;

                // TODO:Ideally would unite enum with flags?
                match abc_type {
                    switch_abc_type_t::SWITCH_ABC_TYPE_INIT => {}
                    switch_abc_type_t::SWITCH_ABC_TYPE_CLOSE => {}
                    switch_abc_type_t::SWITCH_ABC_TYPE_READ => {
                        // Buffer and Send 
                        //tx.send(message)
                    }
                    _ => {}
                };
                true
            });
        },
        ModSubcommand::Stop { session } => {},
    };
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



