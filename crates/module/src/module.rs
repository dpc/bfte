pub mod config;
pub mod db;

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use bfte_consensus_core::citem::{ICitem, IInput, IOutput, ModuleDyn};
use bfte_consensus_core::module::config::ModuleParamsRaw;
use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_consensus_core::ver::{ConsensusVersion, ConsensusVersionMajor, ConsensusVersionMinor};
use bfte_db::Database;
use bfte_util_error::WhateverResult;
use db::{ModuleDatabase, ModuleReadTransaction, ModuleWriteTransactionCtx};
use snafu::Snafu;

use crate::effect::EffectDyn;

#[non_exhaustive]
pub struct ModuleInitArgs {
    pub db: ModuleDatabase,
    pub module_consensus_version: ConsensusVersion,
    pub config: ModuleParamsRaw,
}

pub type DynModuleInit = Arc<dyn ModuleInit + Send + Sync>;

impl ModuleInitArgs {
    pub fn new(
        module_id: ModuleId,
        db: Arc<Database>,
        module_consensus_version: ConsensusVersion,
        config: ModuleParamsRaw,
    ) -> Self {
        Self {
            db: ModuleDatabase::new(module_id, db),
            module_consensus_version,
            config,
        }
    }
}

#[derive(Debug, Snafu)]
pub enum ModuleInitError {
    InvalidConfig,
    UnsupportedVersion,
    Other,
}

pub type ModuleInitResult<T> = Result<T, ModuleInitError>;

/// Module "constructor"
#[async_trait]
pub trait ModuleInit: Any {
    fn kind(&self) -> ModuleKind;

    /// All major consensus version supported by the module, with latest
    /// supported minor version for each
    fn supported_versions(&self) -> BTreeMap<ConsensusVersionMajor, ConsensusVersionMinor>;

    /// Create an instance of module for given arguments
    ///
    /// Note that in principle this might be called multiple times during the
    /// runtime, e.g. because the version changed.
    async fn init(
        &self,
        args: ModuleInitArgs,
    ) -> ModuleInitResult<Arc<dyn IModule + Send + Sync + 'static>>;

    // DELME
    async fn poll(&self) -> Vec<ModuleDyn<dyn ICitem>>;
}

pub type DynModule = Arc<dyn IModule + Send + Sync>;

#[async_trait]
pub trait IModule: Any {
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

    fn process_effect(&self, dbtx: &mut ModuleWriteTransactionCtx, citem: ModuleDyn<dyn ICitem>);
}
