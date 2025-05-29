mod logging;
mod opts;

use std::collections::BTreeMap;
use std::str::FromStr as _;
use std::sync::Arc;

use bfte_consensus_core::module::ModuleKind;
use bfte_consensus_core::ver::ConsensusVersion;
use bfte_derive_secret::DeriveableSecret;
use bfte_module_core::module::ModuleInit;
use bfte_node::Node;
use bfte_node::derive_secret_ext::DeriveSecretExt as _;
use bfte_util_error::WhateverResult;
use clap::Parser as _;
use opts::{Commands, Opts};
use snafu::{OptionExt as _, ResultExt};

pub struct Bfte {
    _something: u32,
}

#[bon::bon]
impl Bfte {
    #[builder(finish_fn = run, start_fn = builder)]
    pub async fn build(
        #[builder(field)] modules_inits: BTreeMap<ModuleKind, Arc<dyn ModuleInit + Send + Sync>>,
    ) -> WhateverResult<()> {
        logging::init_logging()?;

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

                bfte_node::Node::consensus_join_static(db.clone(), &invite)
                    .await
                    .whatever_context("Failed to join consensus")?;

                if !run {
                    return Ok(());
                }

                db
            }

            Commands::Init { extra_peers, run } => {
                let db = Arc::new(
                    Node::open_db(db_path)
                        .await
                        .whatever_context("Failed to open database")?,
                );

                // TODO: get from core module init
                let core_module_init_cons_version = ConsensusVersion::new(0, 0);

                bfte_node::Node::consensus_init_static(
                    db.clone(),
                    secret
                        .whatever_context("Secret must be provided to create a new federation")?,
                    extra_peers,
                    core_module_init_cons_version,
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
            .maybe_force_ui_password(opts.force_ui_password)
            .db(db)
            .ui(Box::new(move |api| {
                Box::pin(async move { bfte_node_ui_axum::run(api, opts.bind_ui).await })
            }))
            .app(Box::new(move |api| {
                Box::pin({
                    let modules_inits = modules_inits.clone();
                    async move { bfte_node_app::NodeApp::new(api, modules_inits).run().await }
                })
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

impl<BS: bfte_build_builder::State> BfteBuildBuilder<BS> {
    pub fn handler(mut self, module_init: Arc<dyn ModuleInit + Send + Sync>) -> Self {
        let kind = module_init.kind();
        if self.modules_inits.insert(kind, module_init).is_some() {
            panic!("Multiple module inits of the same kind {kind}")
        }
        self
    }
}
