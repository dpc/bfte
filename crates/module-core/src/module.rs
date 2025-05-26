use std::collections::BTreeMap;
use std::sync::Arc;

use bfte_consensus_core::citem::{ICitem, IInput, IOutput, ModuleDyn};
use bfte_consensus_core::module::config::ModuleConfigRaw;
use bfte_consensus_core::ver::{ConsensusVersion, ConsensusVersionMajor, ConsensusVersionMinor};
use bfte_util_error::WhateverResult;
use db::{ModuleDb, ModuleReadTransaction, ModuleWriteTransaction};

mod db;

use crate::effect::EffectDyn;

#[non_exhaustive]
pub struct ModuleInitArgs {
    pub db: ModuleDb,
    pub module_consensus_version: ConsensusVersion,
    pub cfg: ModuleConfigRaw,
}

/// Module "constructor"
pub trait ModuleInit {
    /// All major consensus version supported by the module, with latest
    /// supported minor version for each
    fn supported_versions(&self) -> BTreeMap<ConsensusVersionMajor, ConsensusVersionMinor>;

    /// Create an instance of module for given arguments
    ///
    /// Note that in principle this might be called multiple times during the
    /// runtime, e.g. because the version changed.
    fn init(&self, args: ModuleInitArgs) -> Arc<dyn Module + Send + Sync + 'static>;
}

pub trait Module {
    fn process_input(
        &self,
        dbtx: &mut ModuleReadTransaction,
        input: &ModuleDyn<dyn IInput>,
    ) -> WhateverResult<Vec<EffectDyn>>;
    fn process_output(
        &self,
        dbtx: &mut ModuleReadTransaction,
        output: &ModuleDyn<dyn IOutput>,
    ) -> WhateverResult<Vec<EffectDyn>>;
    fn process_citem(
        &self,
        dbtx: &mut ModuleReadTransaction,
        citem: &ModuleDyn<dyn ICitem>,
    ) -> WhateverResult<Vec<EffectDyn>>;

    fn process_effect(&self, dbtx: &mut ModuleWriteTransaction, citem: ModuleDyn<dyn ICitem>);
}
