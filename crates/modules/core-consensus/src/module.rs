use std::collections::BTreeMap;
use std::future;

use async_trait::async_trait;
use bfte_consensus_core::block::BlockRound;
use bfte_consensus_core::citem::{CItemRaw, InputRaw, OutputRaw};
use bfte_consensus_core::module::ModuleId;
use bfte_consensus_core::ver::ConsensusVersion;
use bfte_module::effect::{CItemEffect, ModuleCItemEffect};
use bfte_module::module::IModule;
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::db::{ModuleDatabase, ModuleReadTransaction, ModuleWriteTransactionCtx};
use bfte_util_error::WhateverResult;
use tokio::sync::watch;

use crate::tables;

pub struct CoreConsensusModule {
    #[allow(dead_code)]
    pub(crate) version: ConsensusVersion,
    pub(crate) db: ModuleDatabase,
}

impl CoreConsensusModule {
    pub async fn get_modules_configs(&self) -> BTreeMap<ModuleId, ModuleConfig> {
        self.db
            .read_with_expect(|dbtx| {
                let tbl = dbtx.open_table(&tables::modules_configs::TABLE)?;

                tbl.range(..)?
                    .map(|kv| {
                        let (k, v) = kv?;

                        let module_id = k.value();
                        let value = v.value();
                        Ok((
                            module_id,
                            ModuleConfig {
                                kind: value.kind,
                                version: value.version,
                                params: value.params,
                            },
                        ))
                    })
                    .collect()
            })
            .await
    }
}

#[async_trait]
impl IModule for CoreConsensusModule {
    fn display_name(&self) -> &'static str {
        "Consensus"
    }

    async fn propose_citems_rx(&self) -> watch::Receiver<Vec<CItemRaw>> {
        future::pending().await
    }

    fn process_citem(
        &self,
        _dbtx: &ModuleReadTransaction,
        _round: BlockRound,
        _citem: &CItemRaw,
    ) -> WhateverResult<Vec<CItemEffect>> {
        todo!()
    }

    fn process_input(
        &self,
        _dbtx: &ModuleReadTransaction,
        _input: &InputRaw,
    ) -> WhateverResult<Vec<CItemEffect>> {
        todo!()
    }
    fn process_output(
        &self,
        _dbtx: &ModuleReadTransaction,
        _output: &OutputRaw,
    ) -> WhateverResult<Vec<CItemEffect>> {
        todo!()
    }

    fn process_effects(
        &self,
        _dbtx: &ModuleWriteTransactionCtx,
        _effects: &[ModuleCItemEffect],
    ) -> WhateverResult<()> {
        todo!()
    }
}
