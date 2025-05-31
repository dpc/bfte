// SPDX-License-Identifier: MIT

mod tables;
use std::any::Any;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::mem;
use std::sync::Arc;

use bfte_consensus_core::block::BlockRound;
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_db::Database;
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::{
    DynModule, DynModuleInit, IModule, ModuleInit, ModuleInitArgs, ModuleInitResult,
};
use bfte_module_consensus::ConsensusModule;
use bfte_module_consensus::init::ConsensusModuleInit;
use bfte_node_app_core::NodeAppApi;
use bfte_node_shared_modules::SharedModules;
use bfte_util_error::WhateverResult;
use snafu::{OptionExt as _, ResultExt as _};
use tokio::sync::RwLockWriteGuard;

/// Consensus module is auto-initialized and always there at a fixed id
const CONSENSUS_MODULE_ID: ModuleId = ModuleId::new(0);

pub struct NodeApp {
    db: Arc<Database>,
    node_api: NodeAppApi,
    modules_inits: BTreeMap<ModuleKind, Arc<dyn ModuleInit + Send + Sync>>,
    modules: SharedModules,
    modules_configs: BTreeMap<ModuleId, ModuleConfig>,
}

impl NodeApp {
    pub fn new(
        db: Arc<Database>,
        node_api: NodeAppApi,
        modules_inits: BTreeMap<ModuleKind, DynModuleInit>,
        modules: SharedModules,
    ) -> Self {
        Self {
            node_api,
            modules_inits,
            modules,
            modules_configs: BTreeMap::default(),
            db,
        }
    }

    pub async fn get_cur_round(&self) -> BlockRound {
        self.db
            .read_with_expect(|dbtx| {
                let tbl = dbtx.open_table(&tables::app_cur_round::TABLE)?;

                Ok(tbl.get(&())?.map(|v| v.value()).unwrap_or_default())
            })
            .await
    }

    pub async fn run(mut self) -> WhateverResult<Infallible> {
        let cur_round = self.get_cur_round().await;

        if cur_round == BlockRound::ZERO {
            let consensus_params = self.node_api.get_consensus_params(cur_round).await?;
            self.setup_modules_init(consensus_params).await?;
        } else {
            self.setup_modules().await?;
        }
        loop {
            self.node_api
                .ack_and_wait_next_block(
                    0.into(),
                    Box::pin(async {
                        self.modules_inits
                            .get(&ModuleKind::from(0))
                            .expect("Yes")
                            .poll()
                            .await
                    }),
                )
                .await;
        }
    }

    fn get_consensus_module<'s>(
        &'s self,
        modules_write: &'s RwLockWriteGuard<'_, BTreeMap<ModuleId, DynModule>>,
    ) -> &'s ConsensusModule {
        let consensus_module = modules_write
            .get(&CONSENSUS_MODULE_ID)
            .expect("Must have consensus module");

        (consensus_module.as_ref() as &dyn Any)
            .downcast_ref::<ConsensusModule>()
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

        let default_config = consensus_module_init.init_consensus(consensus_params);

        let new_modules_configs = BTreeMap::from([(CONSENSUS_MODULE_ID, default_config)]);

        Self::setup_modules_to(
            &self.db,
            &mut self.modules_configs,
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
            &mut self.modules_configs,
            modules_write,
            new_modules_configs,
            &self.modules_inits,
        )
        .await
        .whatever_context("Setting up modules failed")
    }

    async fn setup_modules_to(
        db: &Arc<Database>,
        prev_modules_configs: &mut BTreeMap<ModuleId, ModuleConfig>,
        mut modules_write: RwLockWriteGuard<'_, BTreeMap<ModuleId, DynModule>>,
        new_modules_configs: BTreeMap<ModuleId, ModuleConfig>,
        modules_inits: &BTreeMap<ModuleKind, DynModuleInit>,
    ) -> WhateverResult<()> {
        // Put the existing modules aside, to know if all were either reused or
        // destroyed
        let mut existing_modules = mem::take(&mut *modules_write);

        for (module_id, new_module_setup) in new_modules_configs {
            if prev_modules_configs.get(&module_id) == Some(&new_module_setup) {
                // Module config did not change
                modules_write.insert(
                    module_id,
                    existing_modules
                        .remove(&module_id)
                        .expect("Must have an existing module corresponding to existing setup"),
                );
            } else {
                let module_init = modules_inits
                    .get(&new_module_setup.kind)
                    .whatever_context("Missing module init for kind")?;

                // (if any) existing module needs to drop all the resources before starting it
                // again to avoid running two instances at the same time
                existing_modules.remove(&module_id);

                modules_write.insert(
                    module_id,
                    module_init
                        .init(ModuleInitArgs::new(
                            module_id,
                            db.clone(),
                            new_module_setup.version,
                            new_module_setup.config,
                        ))
                        .await
                        .whatever_context("Failed to setup module")?,
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

    async fn init_module(
        db: Arc<Database>,
        module_id: ModuleId,
        new_module_setup: ModuleConfig,
        module_init: &Arc<dyn ModuleInit + Send + Sync>,
    ) -> ModuleInitResult<Arc<dyn IModule + Send + Sync>> {
        module_init
            .init(ModuleInitArgs::new(
                module_id,
                db,
                new_module_setup.version,
                new_module_setup.config,
            ))
            .await
    }
}
