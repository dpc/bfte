mod opts;

use std::io;
use std::str::FromStr as _;
use std::sync::Arc;

use bfte_derive_secret::DeriveableSecret;
use bfte_node::Node;
use bfte_node::derive_secret_ext::DeriveSecretExt as _;
use bfte_util_error::{Whatever, WhateverResult};
use clap::Parser as _;
use opts::{Commands, Opts};
use snafu::{FromString as _, OptionExt as _, ResultExt};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

pub struct Bfte {
    _something: u32,
}

#[bon::bon]
impl Bfte {
    #[builder(finish_fn = run, start_fn = builder)]
    pub async fn build(something: Option<u32>) -> WhateverResult<()> {
        init_logging()?;
        let _ = something;

        let opts = Opts::parse();

        let secret = if let Some(secret_path) = opts.secret_path {
            Some(
                DeriveableSecret::from_str(
                    tokio::fs::read_to_string(secret_path)
                        .await
                        .whatever_context("Failed to read secret file")?
                        .trim(),
                )
                .whatever_context("Failed to parse secret")?,
            )
        } else {
            None
        };

        let db_path = if let Some(data_dir) = opts.data_dir {
            tokio::fs::create_dir_all(&data_dir)
                .await
                .whatever_context("Failed to create/open data dir")?;
            Some(data_dir.join("bfte.redb"))
        } else {
            None
        };

        let db = match opts.command {
            Commands::GenSecret => {
                let root_seckey = DeriveableSecret::generate();
                let peer_seckey = root_seckey.get_peer_seckey().expect("Just generated");
                eprintln!("PeerId: {}", peer_seckey.pubkey());
                eprintln!();
                println!("{}", root_seckey.reveal_display());
                eprintln!();
                eprintln!(
                    "This mnemonic is irrecoverable if lost. Please make a back up before using it!",
                );
                return Ok(());
            }

            Commands::Join { invite, run } => {
                let db = Arc::new(
                    Node::open_db(db_path)
                        .await
                        .whatever_context("Failed to open database")?,
                );

                bfte_node::Node::join(db.clone(), &invite)
                    .await
                    .whatever_context("Failed to join consensus")?;

                if !run {
                    return Ok(());
                }

                db
            }

            Commands::Create { extra_peers, run } => {
                let db = Arc::new(
                    Node::open_db(db_path)
                        .await
                        .whatever_context("Failed to open database")?,
                );

                bfte_node::Node::create(
                    db.clone(),
                    secret
                        .whatever_context("Secret must be provided to create a new federation")?,
                    extra_peers,
                )
                .await
                .whatever_context("Failed to create consensus")?;

                if !run {
                    return Ok(());
                }

                db
            }

            Commands::Run => Arc::new(
                Node::open_db(db_path)
                    .await
                    .whatever_context("Failed to open database")?,
            ),
        };

        bfte_node::Node::builder()
            .maybe_root_secret(secret)
            .db(db)
            .ui(Box::new(move |api| {
                Box::pin(async move { bfte_node_ui_axum::run(api, opts.bind_ui).await })
            }))
            .build()
            .await
            .whatever_context("Failed to build node")?
            .run()
            .await
            .whatever_context("Failed to run node")?;

        Ok(())
    }
}

pub fn init_logging() -> WhateverResult<()> {
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .try_init()
        .map_err(|_| Whatever::without_source("Failed to initialize logging".to_string()))?;

    Ok(())
}
