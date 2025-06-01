// SPDX-License-Identifier: MIT

pub mod init;
mod tables;
use std::collections::BTreeMap;

use async_trait::async_trait;
use bfte_consensus_core::block::BlockRound;
use bfte_consensus_core::citem::{CItemRaw, InputRaw, ModuleDyn, OutputRaw};
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_consensus_core::ver::{ConsensusVersion, ConsensusVersionMajor, ConsensusVersionMinor};
use bfte_module::effect::{CItemEffect, ModuleCItemEffect};
use bfte_module::module::IModule;
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::db::{ModuleDatabase, ModuleReadTransaction, ModuleWriteTransactionCtx};
use bfte_util_error::WhateverResult;
use bincode::{Decode, Encode};

const KIND: ModuleKind = ModuleKind::new(0);
const CURRENT_VERSION: ConsensusVersion =
    ConsensusVersion::new_const(ConsensusVersionMajor::new(0), ConsensusVersionMinor::new(0));

pub struct ConsensusModule {
    db: ModuleDatabase,
}

#[derive(Encode, Decode)]
pub struct ConsensuseModuleParams {
    consensus_params: ConsensusParams,
}

impl ConsensusModule {
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
                                config: value.config,
                            },
                        ))
                    })
                    .collect()
            })
            .await
    }
}

#[async_trait]
impl IModule for ConsensusModule {
    async fn propose_citems(&self) -> Vec<ModuleDyn<CItemRaw>> {
        todo!()
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
