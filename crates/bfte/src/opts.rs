use std::net::SocketAddr;
use std::path::PathBuf;

use bfte_consensus_core::peer::PeerPubkey;
use bfte_invite::Invite;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
pub(crate) struct Opts {
    /// Persist data in an on-disk database, inside a dir
    #[arg(long, env = "BFTE_DATA_DIR", global = true)]
    pub data_dir: Option<PathBuf>,

    /// Force UI password (will be persisted)
    #[arg(long, env = "BFTE_FORCE_UI_PASSWORD", global = true)]
    pub force_ui_password: Option<String>,

    /// Bind UI port
    #[arg(
        long,
        env = "BFTE_BIND_UI",
        default_value = "[::1]:6910",
        global = true
    )]
    pub bind_ui: SocketAddr,

    /// Path to a file containing peer secret key
    #[arg(long, env = "BFTE_SECRET_PATH", global = true)]
    pub secret_path: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    GenSecret,
    Init {
        #[arg(long)]
        run: bool,

        #[arg(long = "extra-peer")]
        extra_peers: Vec<PeerPubkey>,
    },
    Join {
        #[arg(long)]
        run: bool,

        #[arg(long)]
        invite: Invite,
    },
    Run,
}
