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
use bfte_consensus_core::peer_set::PeerSet;
use bfte_db::Database;
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::db::ModuleWriteTransactionCtx;
use bfte_module::module::{DynModuleInit, DynModuleWithConfig, IModuleInit, ModuleInitArgs};
use bfte_module_consensus_ctrl::{ConsensusCtrlModule, ConsensusCtrlModuleInit};
use bfte_node_app_core::NodeAppApi;
use bfte_node_shared_modules::SharedModules;
use bfte_util_error::WhateverResult;
use snafu::{OptionExt as _, ResultExt as _};
use tables::BlockCItemIdx;
use tokio::sync::{RwLockWriteGuard, watch};
use tracing::{debug, info};

/// Consensus module is auto-initialized and always there at a fixed id
const CONSENSUS_CTRL_MODULE_ID: ModuleId = ModuleId::new(0);
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
    #[allow(dead_code)] // will get there
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
            modules_inits.contains_key(&bfte_module_consensus_ctrl::KIND),
            "modules_inits must have ConsensusCtrlModuleInit"
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

        let consensus_params = self.node_api.get_consensus_params(cur_round_idx.0).await;

        {
            let init_modules_configs = self.init_consensus_ctrl(consensus_params).await?;
            let changed = self.setup_modules_to(&init_modules_configs).await?;

            assert!(changed);
        }

        let mut modules_configs = None;
        let mut peer_set = None;

        self.record_supported_modules_versions().await;
        info!(
           target: LOG_TARGET,
           round = %cur_round_idx.0,
           citem_idx = %cur_round_idx.1,
           "Started node app"
        );

        loop {
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
                    self.reload_invalidated_copies(&mut modules_configs, &mut peer_set)
                        .await?;

                    self.process_citem(
                        cur_round_idx,
                        &block_header,
                        peer_pubkey,
                        &mut peer_set,
                        &mut modules_configs,
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

    async fn reload_invalidated_copies(
        &self,
        modules_configs: &mut Option<BTreeMap<ModuleId, ModuleConfig>>,
        peer_set: &mut Option<PeerSet>,
    ) -> Result<(), bfte_util_error::Whatever> {
        if peer_set.is_none() {
            *peer_set = Some(
                Self::consensus_ctrl_module_expect_static(&self.modules.read().await)
                    .get_peer_set()
                    .await,
            );
        }
        if modules_configs.is_none() {
            *modules_configs = Some(
                Self::consensus_ctrl_module_expect_static(&self.modules.read().await)
                    .get_modules_configs()
                    .await,
            );

            // Reload modules in case of any changes
            self.setup_modules_to(modules_configs.as_ref().expect("Must be set"))
                .await?;
        }
        debug_assert_eq!(
            *peer_set.as_ref().expect("Must be set"),
            Self::consensus_ctrl_module_expect_static(&self.modules.read().await)
                .get_peer_set()
                .await
        );
        debug_assert_eq!(
            *modules_configs.as_ref().expect("Must be set"),
            Self::consensus_ctrl_module_expect_static(&self.modules.read().await)
                .get_modules_configs()
                .await
        );
        Ok(())
    }

    fn consensus_ctrl_module_expect_static(
        modules: &impl ops::Deref<Target = BTreeMap<ModuleId, DynModuleWithConfig>>,
    ) -> &ConsensusCtrlModule {
        let consensus_module = modules
            .get(&CONSENSUS_CTRL_MODULE_ID)
            .expect("Must have a app consensus module");

        (consensus_module.inner.as_ref() as &dyn Any)
            .downcast_ref::<ConsensusCtrlModule>()
            .expect("Must be a core consensus module")
    }

    /// Setup modules to initial (fresh consensus) position
    async fn init_consensus_ctrl(
        &mut self,
        consensus_params: ConsensusParams,
    ) -> WhateverResult<BTreeMap<ModuleId, ModuleConfig>> {
        let modules_write = self.modules.write().await;
        assert!(modules_write.is_empty());
        let consensus_module_init = self
            .modules_inits
            .get(&ConsensusCtrlModuleInit.kind())
            .whatever_context("Missing module init for consensus module kind")?;
        let consensus_module_init = (consensus_module_init.as_ref() as &dyn Any)
            .downcast_ref::<ConsensusCtrlModuleInit>()
            .expect("Must be a consensus module");

        Ok(self
            .db
            .write_with_expect(|dbtx| {
                let dbtx = &ModuleWriteTransactionCtx::new(CONSENSUS_CTRL_MODULE_ID, dbtx);
                if consensus_module_init.is_bootstrapped(dbtx)? {
                    debug!(target: LOG_TARGET, "ConsensusCtrl already bootstrapped");
                    consensus_module_init.get_modules_configs(dbtx)
                } else {
                    debug!(target: LOG_TARGET, "Bootstrapping ConsensusCtrl from `consensus_params`");
                    let default_config = consensus_module_init.bootstrap_consensus(
                        dbtx,
                        CONSENSUS_CTRL_MODULE_ID,
                        consensus_params.init_core_module_cons_version,
                        consensus_params.peers.clone(),
                    )?;

                    Ok(BTreeMap::from([(CONSENSUS_CTRL_MODULE_ID, default_config)]))
                }
            })
            .await
)
    }

    async fn record_supported_modules_versions(&self) {
        // Collect supported versions from all module inits
        let mut modules_supported_versions = BTreeMap::new();
        for (module_kind, module_init) in &self.modules_inits {
            modules_supported_versions.insert(*module_kind, module_init.supported_versions());
        }

        let modules_read = self.modules.read().await;
        let consensus_ctrl = Self::consensus_ctrl_module_expect_static(&modules_read);
        consensus_ctrl
            .record_module_init_versions(&modules_supported_versions)
            .await;
    }

    async fn setup_modules_to(
        &self,
        new_modules_configs: &BTreeMap<ModuleId, ModuleConfig>,
    ) -> WhateverResult<bool> {
        let modules = self.modules.write().await;
        let changed = Self::setup_modules_to_static(
            &self.db,
            modules,
            new_modules_configs,
            &self.modules_inits,
            self.peer_pubkey,
        )
        .await?;

        if changed {
            self.modules.send_changed();
        }
        Ok(changed)
    }

    #[must_use = "Don't forget to send update"]
    async fn setup_modules_to_static(
        db: &Arc<Database>,
        mut modules_write: RwLockWriteGuard<'_, BTreeMap<ModuleId, DynModuleWithConfig>>,
        new_modules_configs: &BTreeMap<ModuleId, ModuleConfig>,
        modules_inits: &BTreeMap<ModuleKind, DynModuleInit>,
        peer_pubkey: Option<PeerPubkey>,
    ) -> WhateverResult<bool> {
        // Put the existing modules aside, to know if all were either reused or
        // destroyed
        let mut existing_modules = mem::take(&mut *modules_write);

        let mut changed = false;

        for (module_id, new_module_config) in new_modules_configs {
            if existing_modules.get(module_id).map(|module| &module.config)
                == Some(new_module_config)
            {
                // Module config did not change
                modules_write.insert(
                    *module_id,
                    existing_modules
                        .remove(module_id)
                        .expect("Must have an existing module corresponding to existing setup"),
                );
            } else {
                changed = true;

                let module_init = modules_inits
                    .get(&new_module_config.kind)
                    .whatever_context("Missing module init for kind")?;

                // (if any) existing module needs to drop all the resources before starting it
                // again to avoid running two instances at the same time
                existing_modules.remove(module_id);

                debug!(target: LOG_TARGET, %module_id, config = ?new_module_config, "Initializing module");
                modules_write.insert(
                    *module_id,
                    DynModuleWithConfig {
                        config: new_module_config.clone(),
                        inner: module_init
                            .init(ModuleInitArgs::new(
                                *module_id,
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
