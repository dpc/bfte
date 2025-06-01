use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use bfte_consensus_core::bincode::CONSENSUS_BINCODE_CONFIG;
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_consensus_core::ver::{ConsensusVersionMajor, ConsensusVersionMinor};
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::db::{DbResult, ModuleWriteTransactionCtx};
use bfte_module::module::{
    IModule, ModuleInit, ModuleInitArgs, ModuleInitResult, UnsupportedVersionSnafu,
};
use snafu::ensure;

use crate::tables::modules_configs;
use crate::{CURRENT_VERSION, KIND};

pub struct ConsensusModuleInit;

impl ConsensusModuleInit {
    /// Initialize consensus module
    ///
    /// Since consensus module is the one storing consensus configs for itself
    /// and other modules, it needs to be initialized manually.
    ///
    /// Its own [`ModuleConfig`] it was initialized with
    pub fn init_consensus(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        module_id: ModuleId,
        consensus_params: ConsensusParams,
    ) -> DbResult<ModuleConfig> {
        let config = ModuleConfig {
            kind: KIND,
            version: CURRENT_VERSION,
            params: bincode::encode_to_vec(
                &super::params::ConsensusModuleParams { consensus_params },
                CONSENSUS_BINCODE_CONFIG,
            )
            .expect("Can't fail")
            .into(),
        };

        let mut tbl = dbtx.open_table(&modules_configs::TABLE)?;
        tbl.insert(&module_id, &config)?;

        Ok(config)
    }
}

#[async_trait]
impl ModuleInit for ConsensusModuleInit {
    fn kind(&self) -> ModuleKind {
        crate::KIND
    }

    /// All major consensus version supported by the module, with latest
    /// supported minor version for each
    fn supported_versions(&self) -> BTreeMap<ConsensusVersionMajor, ConsensusVersionMinor> {
        todo!()
    }

    /// Create an instance of module for given arguments
    ///
    /// Note that in principle this might be called multiple times during the
    /// runtime, e.g. because the version changed.
    async fn init(
        &self,
        args: ModuleInitArgs,
    ) -> ModuleInitResult<Arc<dyn IModule + Send + Sync + 'static>> {
        ensure!(
            args.module_consensus_version <= super::CURRENT_VERSION,
            UnsupportedVersionSnafu {
                requested: args.module_consensus_version,
                supported: super::CURRENT_VERSION
            }
        );

        Ok(Arc::new(super::ConsensusModule {
            db: args.db,
            version: args.module_consensus_version,
        }))
    }
}
