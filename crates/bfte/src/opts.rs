use std::net::SocketAddr;
use std::path::PathBuf;

use bfte_consensus_core::peer::PeerPubkey;
use bfte_invite::Invite;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
pub(crate) struct Opts {
    #[arg(long, env = "BFTE_DATA_DIR", global = true)]
    pub data_dir: Option<PathBuf>,

    #[arg(
        long,
        env = "BFTE_DATA_DIR",
        default_value = "[::1]:6910",
        global = true
    )]
    pub bind_ui: SocketAddr,

    #[arg(long, env = "BFTE_SECRET_PATH", global = true)]
    pub secret_path: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    GenSecret,
    Create {
        #[arg(long, default_value = "false")]
        run: bool,

        #[arg(long = "extra-peer")]
        extra_peers: Vec<PeerPubkey>,
    },
    Join {
        #[arg(long, default_value = "false")]
        run: bool,

        #[arg(long)]
        invite: Invite,
    },
    Run,
}
