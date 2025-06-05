// SPDX-License-Identifier: MIT

mod db;
mod init;
mod process_citem;
mod tables;

use std::any::Any;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::mem;
use std::sync::Arc;

use bfte_consensus_core::citem::transaction::Transaction;
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_db::Database;
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::db::ModuleWriteTransactionCtx;
use bfte_module::module::{DynModuleInit, DynModuleWithConfig, ModuleInit, ModuleInitArgs};
use bfte_module_core_consensus::{ConsensusModuleInit, CoreConsensusModule};
use bfte_node_app_core::NodeAppApi;
use bfte_node_shared_modules::SharedModules;
use bfte_util_error::WhateverResult;
use snafu::{OptionExt as _, ResultExt as _};
use tables::BlockCItemIdx;
use tokio::sync::{RwLockWriteGuard, watch};
use tracing::{debug, info};

/// Consensus module is auto-initialized and always there at a fixed id
const CONSENSUS_MODULE_ID: ModuleId = ModuleId::new(0);
const LOG_TARGET: &str = "bfte::app";

pub type ModulesInits = BTreeMap<ModuleKind, DynModuleInit>;

/// Node's application layer
///
/// Once node/consensus finalizes blocks, they are asynchronously processed
/// by this object/actor.
pub struct NodeApp {
    /// Database storing tables of this and other core modules
    db: Arc<Database>,

    /// Api to call [`bfte-node`]
    node_api: NodeAppApi,

    /// Uses for creating instances of modules of a given kind
    modules_inits: BTreeMap<ModuleKind, Arc<dyn ModuleInit + Send + Sync>>,

    /// All initialized modules
    ///
    /// This uses a special struct, as a read-only weakly-owned view
    /// of this is shared with `bfte-node`.
    modules: SharedModules,

    /// Used to signal pending transactions
    pending_transactions_tx: watch::Sender<Vec<Transaction>>,
}

impl NodeApp {
    pub fn new(
        db: Arc<Database>,
        node_api: NodeAppApi,
        mut modules_inits: ModulesInits,
        modules: SharedModules,
        pending_transactions_tx: watch::Sender<Vec<Transaction>>,
    ) -> Self {
        modules_inits
            .entry(bfte_module_core_consensus::KIND)
            .or_insert_with(|| Arc::new(bfte_module_core_consensus::init::ConsensusModuleInit));
        Self {
            node_api,
            modules_inits,
            modules,
            db,
            pending_transactions_tx,
        }
    }

    /// Main loop which processes consensus items ([`CItem`]s) from each
    /// finalized block as they become available.
    pub async fn run(mut self) -> WhateverResult<Infallible> {
        self.db.write_with_expect(Self::init_tables_tx).await;

        let mut cur_round_idx = self.load_cur_round_and_idx().await;

        if cur_round_idx == Default::default() {
            info!(target: LOG_TARGET, ?cur_round_idx, "Initializing app level consensus processing...");
            let consensus_params = self.node_api.get_consensus_params(cur_round_idx.0).await;
            self.setup_modules_init(consensus_params).await?;
        } else {
            info!(target: LOG_TARGET, ?cur_round_idx, "Started node app level processing");
            self.setup_modules().await?;
        }

        loop {
            debug!(target: LOG_TARGET, ?cur_round_idx, "Awaiting next round...");
            let (block_header, citems) =
                self.node_api.ack_and_wait_next_block(cur_round_idx.0).await;
            debug!(target: LOG_TARGET, round = %block_header.round, "Processing new block...");

            for (idx, citem) in citems.iter().enumerate() {
                let idx = BlockCItemIdx::from(u32::try_from(idx).expect("Can't fail"));
                if cur_round_idx.1 <= idx {
                    self.process_citem(cur_round_idx, &block_header, citem)
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
                    Self::save_cur_round_and_idx(dbtx, cur_round_idx.0, cur_round_idx.1)
                })
                .await;
        }
    }

    fn get_consensus_module<'s>(
        &'s self,
        modules_write: &'s RwLockWriteGuard<'_, BTreeMap<ModuleId, DynModuleWithConfig>>,
    ) -> &'s CoreConsensusModule {
        let consensus_module = modules_write
            .get(&CONSENSUS_MODULE_ID)
            .expect("Must have consensus module");

        (consensus_module.inner.as_ref() as &dyn Any)
            .downcast_ref::<CoreConsensusModule>()
            .expect("Must be a consensus module")
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
            .get(&ConsensusModuleInit.kind())
            .whatever_context("Missing module init for consensus module kind")?;
        let consensus_module_init = (consensus_module_init.as_ref() as &dyn Any)
            .downcast_ref::<ConsensusModuleInit>()
            .expect("Must be a consensus module");
        let default_config = self
            .db
            .write_with_expect(|dbtx| {
                let dbtx = &ModuleWriteTransactionCtx::new(CONSENSUS_MODULE_ID, dbtx);

                consensus_module_init.init_consensus(dbtx, CONSENSUS_MODULE_ID, consensus_params)
            })
            .await;
        let new_modules_configs = BTreeMap::from([(CONSENSUS_MODULE_ID, default_config)]);
        Self::setup_modules_to(
            &self.db,
            modules_write,
            new_modules_configs,
            &self.modules_inits,
        )
        .await
        .whatever_context("Setting up modules failed")
    }

    /// Setup modules using existing settings tracked by the consensus module
    async fn setup_modules(&mut self) -> WhateverResult<()> {
        let modules_write = self.modules.write().await;
        let consensus_module = self.get_consensus_module(&modules_write);

        let new_modules_configs = consensus_module.get_modules_configs().await;

        Self::setup_modules_to(
            &self.db,
            modules_write,
            new_modules_configs,
            &self.modules_inits,
        )
        .await
        .whatever_context("Setting up modules failed")
    }

    async fn setup_modules_to(
        db: &Arc<Database>,
        mut modules_write: RwLockWriteGuard<'_, BTreeMap<ModuleId, DynModuleWithConfig>>,
        new_modules_configs: BTreeMap<ModuleId, ModuleConfig>,
        modules_inits: &BTreeMap<ModuleKind, DynModuleInit>,
    ) -> WhateverResult<()> {
        // Put the existing modules aside, to know if all were either reused or
        // destroyed
        let mut existing_modules = mem::take(&mut *modules_write);

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
                let module_init = modules_inits
                    .get(&new_module_config.kind)
                    .whatever_context("Missing module init for kind")?;

                // (if any) existing module needs to drop all the resources before starting it
                // again to avoid running two instances at the same time
                existing_modules.remove(&module_id);

                modules_write.insert(
                    module_id,
                    DynModuleWithConfig {
                        config: new_module_config.clone(),
                        inner: module_init
                            .init(ModuleInitArgs::new(
                                module_id,
                                db.clone(),
                                new_module_config.version,
                                new_module_config.params,
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
        Ok(())
    }
}
