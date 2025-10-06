use anyhow::Result;
use clap::{Command, FromArgMatches as _, Parser, Subcommand as _};

#[derive(Parser, Debug)]
pub(crate) struct Common {
    pub session_id: String,
    pub bug_name: Option<String>,
}

#[derive(Parser, Debug)]
pub(crate) enum Subcommands {
    Start {
        url: String,
        #[command(flatten)]
        fork: Common,
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
        text: String,
        #[command(flatten)]
        fork: Common,
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
