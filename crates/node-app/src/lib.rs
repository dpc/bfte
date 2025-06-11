// SPDX-License-Identifier: MIT

//! Application level node logic
//!
//! "Application level" can be understood as the layer above "core" layer -
//! consuming what the core consensus agreed on (finalized) between the peers,
//! and sending it new things to agree on.
mod db;
mod init;
mod process_citem;
mod tables;

use std::any::Any;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::{mem, ops};

use bfte_consensus::consensus::Consensus;
use bfte_consensus_core::citem::transaction::Transaction;
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_consensus_core::peer::PeerPubkey;
use bfte_db::Database;
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::db::ModuleWriteTransactionCtx;
use bfte_module::module::{DynModuleInit, DynModuleWithConfig, IModuleInit, ModuleInitArgs};
use bfte_module_app_consensus::{AppConsensusModule, AppConsensusModuleInit};
use bfte_node_app_core::NodeAppApi;
use bfte_node_shared_modules::SharedModules;
use bfte_util_error::WhateverResult;
use snafu::{OptionExt as _, ResultExt as _};
use tables::BlockCItemIdx;
use tokio::sync::{RwLockWriteGuard, watch};
use tracing::{debug, info};

/// Consensus module is auto-initialized and always there at a fixed id
const APP_CONSENSUS_MODULE_ID: ModuleId = ModuleId::new(0);
const LOG_TARGET: &str = "bfte::app";

pub type ModulesInits = BTreeMap<ModuleKind, DynModuleInit>;

/// Node's application layer
///
/// Once node/consensus finalizes blocks, they are asynchronously processed
/// by this object/actor.
pub struct NodeApp {
    /// Database storing tables of this and other core modules
    db: Arc<Database>,

    /// Direct reference to the consensus
    ///
    /// Should only be used to schedule consensus param changes.
    consensus: Arc<Consensus>,

    /// Api to call [`bfte-node`]
    node_api: NodeAppApi,

    /// Uses for creating instances of modules of a given kind
    modules_inits: BTreeMap<ModuleKind, DynModuleInit>,

    /// All initialized modules
    ///
    /// This uses a special struct, as a read-only weakly-owned view
    /// of this is shared with `bfte-node`.
    modules: SharedModules,

    /// Used to signal pending transactions
    pending_transactions_tx: watch::Sender<Vec<Transaction>>,

    peer_pubkey: Option<PeerPubkey>,
}

impl NodeApp {
    pub async fn new(
        db: Arc<Database>,
        node_api: NodeAppApi,
        modules_inits: ModulesInits,
        modules: SharedModules,
        pending_transactions_tx: watch::Sender<Vec<Transaction>>,
    ) -> Self {
        assert!(
            modules_inits.contains_key(&bfte_module_app_consensus::KIND),
            "modules_inits must have AppConsensusModuleInit"
        );
        let peer_pubkey = node_api.get_peer_pubkey().await;
        let consensus = node_api.get_consensus().await;

        db.write_with_expect(Self::init_tables_dbtx).await;

        Self {
            node_api,
            modules_inits,
            modules,
            db,
            pending_transactions_tx,
            peer_pubkey,
            consensus,
        }
    }

    /// Main loop which processes consensus items ([`CItem`]s) from each
    /// finalized block as they become available.
    pub async fn run(mut self) -> WhateverResult<Infallible> {
        let mut cur_round_idx = self.load_cur_round_and_idx().await;

        let mut peer_set = if cur_round_idx == Default::default() {
            info!(target: LOG_TARGET, ?cur_round_idx, "Initializing app level consensus processing…");
            let consensus_params = self.node_api.get_consensus_params(cur_round_idx.0).await;
            self.setup_modules_init(consensus_params.clone()).await?;

            consensus_params.peers
        } else {
            info!(
               target: LOG_TARGET,
               round = %cur_round_idx.0,
               citem_idx = %cur_round_idx.1,
               "Started node app consensus"
            );
            self.setup_modules().await?;
            self.db
                .read_with_expect(|dbtx| {
                    Ok(dbtx
                        .open_table(&tables::app_cur_peer_set::TABLE)?
                        .get(&())?
                        .expect("Must have a peer set")
                        .value())
                })
                .await
        };

        self.record_modules_versions().await;

        loop {
            // Reload modules in case of any changes
            self.setup_modules().await?;

            debug!(
                target: LOG_TARGET,
                round = %cur_round_idx.0,
                citem_idx = %cur_round_idx.1,
                "Awaiting block data…"
            );
            let (block_header, peer_pubkey, citems) =
                self.node_api.ack_and_wait_next_block(cur_round_idx.0).await;
            debug!(target: LOG_TARGET, round = %block_header.round, "Processing new block…");

            for (idx, citem) in citems.iter().enumerate() {
                let idx = BlockCItemIdx::from(u32::try_from(idx).expect("Can't fail"));
                if cur_round_idx.1 <= idx {
                    self.process_citem(
                        cur_round_idx,
                        &block_header,
                        peer_pubkey,
                        &mut peer_set,
                        citem,
                    )
                    .await;
                }
                cur_round_idx.1 = idx;
            }

            cur_round_idx = (
                block_header.round.next().expect("Can't fail"),
                BlockCItemIdx::new(0),
            );
            self.db
                .write_with_expect(|dbtx| {
                    Self::save_cur_round_and_idx_dbtx(dbtx, cur_round_idx.0, cur_round_idx.1)
                })
                .await;
        }
    }

    fn get_app_consensus_module<'s>(
        &'s self,
        modules: &'s impl ops::Deref<Target = BTreeMap<ModuleId, DynModuleWithConfig>>,
    ) -> &'s AppConsensusModule {
        let consensus_module = modules
            .get(&APP_CONSENSUS_MODULE_ID)
            .expect("Must have a app consensus module");

        (consensus_module.inner.as_ref() as &dyn Any)
            .downcast_ref::<AppConsensusModule>()
            .expect("Must be a core consensus module")
    }

    /// Setup modules to initial (fresh consensus) position
    async fn setup_modules_init(
        &mut self,
        consensus_params: ConsensusParams,
    ) -> WhateverResult<()> {
        let modules_write = self.modules.write().await;
        assert!(modules_write.is_empty());
        let consensus_module_init = self
            .modules_inits
            .get(&AppConsensusModuleInit.kind())
            .whatever_context("Missing module init for consensus module kind")?;
        let consensus_module_init = (consensus_module_init.as_ref() as &dyn Any)
            .downcast_ref::<AppConsensusModuleInit>()
            .expect("Must be a consensus module");

        let new_modules_configs = self
            .db
            .write_with_expect(|dbtx| {
                let dbtx = &ModuleWriteTransactionCtx::new(APP_CONSENSUS_MODULE_ID, dbtx);
                if consensus_module_init.is_bootstrapped(dbtx)? {
                    debug!(target: LOG_TARGET, "AppConsensus already bootstrapped");
                    consensus_module_init.get_modules_configs_dbtx(dbtx)
                } else {
                    debug!(target: LOG_TARGET, "Bootstrapping AppConsensus from `consensus_params`");
                    let default_config = consensus_module_init.bootstrap_consensus(
                        dbtx,
                        APP_CONSENSUS_MODULE_ID,
                        consensus_params.init_core_module_cons_version,
                        consensus_params.peers.clone(),
                    )?;

                    dbtx.open_table(&tables::app_cur_peer_set::TABLE)?.insert(&(), &consensus_params.peers)?;

                    Ok(BTreeMap::from([(APP_CONSENSUS_MODULE_ID, default_config)]))
                }
            })
            .await;

        if Self::setup_modules_to(
            &self.db,
            modules_write,
            new_modules_configs,
            &self.modules_inits,
            self.peer_pubkey,
        )
        .await
        .whatever_context("Setting up modules failed")?
        {
            self.modules.send_changed();
        }

        Ok(())
    }

    /// Setup modules using existing settings tracked by the consensus module
    async fn setup_modules(&mut self) -> WhateverResult<()> {
        let modules_write = self.modules.write().await;

        let new_modules_configs = if modules_write.contains_key(&APP_CONSENSUS_MODULE_ID) {
            self.get_app_consensus_module(&modules_write)
                .get_modules_configs()
                .await
        } else {
            // In case we don't have the core consensus module initialized yet,
            // we use special function on the init.
            let consensus_module_init = self
                .modules_inits
                .get(&AppConsensusModuleInit.kind())
                .whatever_context("Missing module init for consensus module kind")?;
            let consensus_module_init = (consensus_module_init.as_ref() as &dyn Any)
                .downcast_ref::<AppConsensusModuleInit>()
                .expect("Must be a consensus module");
            self.db
                .write_with_expect(|dbtx| {
                    let dbtx = &ModuleWriteTransactionCtx::new(APP_CONSENSUS_MODULE_ID, dbtx);
                    consensus_module_init.get_modules_configs_dbtx(dbtx)
                })
                .await
        };
        if Self::setup_modules_to(
            &self.db,
            modules_write,
            new_modules_configs,
            &self.modules_inits,
            self.peer_pubkey,
        )
        .await
        .whatever_context("Setting up modules failed")?
        {
            self.modules.send_changed();
        }
        Ok(())
    }

    async fn record_modules_versions(&self) {
        // Collect supported versions from all module inits
        let mut modules_supported_versions = BTreeMap::new();
        for (module_kind, module_init) in &self.modules_inits {
            modules_supported_versions.insert(*module_kind, module_init.supported_versions());
        }

        let modules_read = self.modules.read().await;
        let app_consensus = self.get_app_consensus_module(&modules_read);
        app_consensus
            .record_module_init_versions(&modules_supported_versions)
            .await;
    }

    #[must_use = "Don't forget to send update"]
    async fn setup_modules_to(
        db: &Arc<Database>,
        mut modules_write: RwLockWriteGuard<'_, BTreeMap<ModuleId, DynModuleWithConfig>>,
        new_modules_configs: BTreeMap<ModuleId, ModuleConfig>,
        modules_inits: &BTreeMap<ModuleKind, DynModuleInit>,
        peer_pubkey: Option<PeerPubkey>,
    ) -> WhateverResult<bool> {
        // Put the existing modules aside, to know if all were either reused or
        // destroyed
        let mut existing_modules = mem::take(&mut *modules_write);

        let mut changed = false;

        for (module_id, new_module_config) in new_modules_configs {
            if existing_modules
                .get(&module_id)
                .map(|module| &module.config)
                == Some(&new_module_config)
            {
                // Module config did not change
                modules_write.insert(
                    module_id,
                    existing_modules
                        .remove(&module_id)
                        .expect("Must have an existing module corresponding to existing setup"),
                );
            } else {
                changed = true;

                let module_init = modules_inits
                    .get(&new_module_config.kind)
                    .whatever_context("Missing module init for kind")?;

                // (if any) existing module needs to drop all the resources before starting it
                // again to avoid running two instances at the same time
                existing_modules.remove(&module_id);

                debug!(target: LOG_TARGET, %module_id, config = ?new_module_config, "Initializing module");
                modules_write.insert(
                    module_id,
                    DynModuleWithConfig {
                        config: new_module_config.clone(),
                        inner: module_init
                            .init(ModuleInitArgs::new(
                                module_id,
                                db.clone(),
                                new_module_config.version,
                                modules_inits.clone(),
                                peer_pubkey,
                            ))
                            .await
                            .whatever_context("Failed to setup module")?,
                    },
                );
            }
        }
        assert!(
            existing_modules.is_empty(),
            "Some existing modules are without config in the new round: {:?}",
            existing_modules.keys()
        );
        Ok(changed)
    }
}
