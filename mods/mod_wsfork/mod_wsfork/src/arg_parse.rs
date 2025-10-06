use anyhow::Result;
use clap::{Command, FromArgMatches as _, Parser, Subcommand as _};

#[derive(Parser, Debug)]
pub(crate) struct Common {
    pub session_id: String,
    pub bug_name: String,
}

#[derive(Parser, Debug)]
pub(crate) enum Subcommands {
    Start {
        #[command(flatten)]
        fork: Common,
        url: String,
        #[arg(default_value_t = false)]
        start_paused: bool,
    },
    Stop {
        #[command(flatten)]
        fork: Common,
    },
    Pause {
        #[command(flatten)]
        fork: Common,
    },
    Resume {
        #[command(flatten)]
        fork: Common,
    },
    SendText {
        #[command(flatten)]
        fork: Common,
        text: String,
    },
}

impl Subcommands {
    pub(crate) fn common_args(&self) -> &Common {
        match self {
            Self::SendText { fork, .. } => fork,
            Self::Start { fork, .. } => fork,
            Self::Stop { fork, .. } => fork,
            Self::Pause { fork, .. } => fork,
            Self::Resume { fork, .. } => fork,
        }
    }
}

pub(crate) fn parse_args(cmd_str: &str) -> Result<Subcommands> {
    let mut cmd = Command::new("wsfork")
        .disable_version_flag(true)
        .disable_help_flag(true)
        .ignore_errors(true)
        .no_binary_name(true);
    cmd = Subcommands::augment_subcommands(cmd);
    let matches = cmd.try_get_matches_from(cmd_str.split(' '))?;
    let s = Subcommands::from_arg_matches(&matches)?;
    Ok(s)
}
