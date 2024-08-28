use std::borrow::Cow;
use clap::ArgAction;
use clap::value_parser;
use clap::{Command, Arg};
use freeswitch_rs::Stream;
use freeswitch_sys::switch_abc_type_t;
use freeswitch_rs::Session;
use tokio::runtime::Runtime;
use std::sync::OnceLock;
use freeswitch_rs::switch_module_define;
use freeswitch_rs::switch_api_define;
use freeswitch_rs::LoadableModule;

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

#[switch_module_define]
struct ModAudioFork;

impl LoadableModule for ModAudioFork {
    fn load(builder:freeswitch_rs::ModuleBuilder) -> bool {
        let _ = RT.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .build()
                .unwrap()
        });
        builder.add(api_main);
        return true
    }
}

#[switch_api_define("AudioFork")]
fn api_main(cmd:&str, session:Option<Session>, stream:Stream) {
    let res = match parse_args(cmd) {
        Err(e) => Ok(()),
        Ok(cmd) => {
            match cmd {
                ModSubcommand::Start { session, url } => api_start(session, url),
                ModSubcommand::Stop { session } => Ok(())
            }
        }
    };
}

fn api_start(uuid:String, url:String) -> Result<(),Error> {
    // We can locate session and have RAII guard unlock for us when scope finishes
    let s = Session::locate(&uuid).ok_or(Error::InvalidArguments)?;

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
    Ok(())
}

fn parse_args(cmd_str:&str) -> Result<ModSubcommand,Error> {
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



