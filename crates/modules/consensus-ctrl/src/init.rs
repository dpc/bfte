use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_consensus_core::peer_set::PeerSet;
use bfte_consensus_core::ver::{ConsensusVersion, ConsensusVersionMajor, ConsensusVersionMinor};
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::db::{DbResult, ModuleReadableTransaction, ModuleWriteTransactionCtx};
use bfte_module::module::{
    IModule, IModuleInit, ModuleInitArgs, ModuleInitResult, UnsupportedVersionSnafu,
};
use snafu::ensure;
use tokio::sync::watch;
use tracing::debug;

use super::ConsensusCtrlModule;
use crate::tables::{self, modules_configs};
use crate::{CURRENT_VERSION_MAJOR, CURRENT_VERSION_MINOR, KIND, LOG_TARGET};

pub struct ConsensusCtrlModuleInit;

impl ConsensusCtrlModuleInit {
    pub fn is_bootstrapped(&self, dbtx: &ModuleWriteTransactionCtx) -> DbResult<bool> {
        let tbl = dbtx.open_table(&tables::self_version::TABLE)?;

        Ok(tbl.get(&())?.is_some())
    }

    /// Initialize consensus module
    ///
    /// Since consensus module is the one storing consensus configs for itself
    /// and other modules, it needs to be initialized manually on first run.
    pub fn bootstrap_consensus(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        module_id: ModuleId,
        version: ConsensusVersion,
        peer_set: PeerSet,
    ) -> DbResult<ModuleConfig> {
        let config = ModuleConfig {
            kind: KIND,
            version,
        };

        debug!(target: LOG_TARGET, %version, "Bootstrapping consensus with initial ConsensusCtrl module");

        {
            let mut tbl = dbtx.open_table(&tables::self_version::TABLE)?;
            assert!(tbl.insert(&(), &version)?.is_none());
        }

        {
            let mut tbl = dbtx.open_table(&modules_configs::TABLE)?;
            tbl.insert(&module_id, &config)?;
        }

        {
            let mut tbl = dbtx.open_table(&tables::peers::TABLE)?;
            for peer in peer_set {
                tbl.insert(&peer, &())?;
            }
        }

        Ok(config)
    }

    pub fn get_modules_configs<'s>(
        &self,
        dbtx: &impl ModuleReadableTransaction<'s>,
    ) -> DbResult<BTreeMap<ModuleId, ModuleConfig>> {
        ConsensusCtrlModule::get_modules_configs_dbtx(dbtx)
    }
}

#[async_trait]
impl IModuleInit for ConsensusCtrlModuleInit {
    fn kind(&self) -> ModuleKind {
        crate::KIND
    }

    fn singleton(&self) -> bool {
        true
    }

    fn display_name(&self) -> &'static str {
        "Consensus Ctrl"
    }

    /// All major consensus version supported by the module, with latest
    /// supported minor version for each
    fn supported_versions(&self) -> BTreeMap<ConsensusVersionMajor, ConsensusVersionMinor> {
        BTreeMap::from([(CURRENT_VERSION_MAJOR, CURRENT_VERSION_MINOR)])
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
            args.module_consensus_version.major() == super::CURRENT_VERSION_MAJOR
                && args.module_consensus_version.minor() <= super::CURRENT_VERSION_MINOR,
            UnsupportedVersionSnafu {
                requested: args.module_consensus_version,
                supported: ConsensusVersion::new(
                    super::CURRENT_VERSION_MAJOR,
                    super::CURRENT_VERSION_MINOR
                ),
            }
        );

        let (propose_citems_tx, propose_citems_rx) = watch::channel(Vec::new());

        args.db
            .write_with_expect(|dbtx| {
                ConsensusCtrlModule::init_db_tx(dbtx, args.module_consensus_version)
            })
            .await;

        let module = ConsensusCtrlModule {
            db: args.db,
            version: args.module_consensus_version,
            peer_pubkey: args.peer_pubkey,
            propose_citems_rx,
            propose_citems_tx,
            modules_inits: args.modules_inits,
        };

        module.refresh_consensus_proposals().await;

        Ok(Arc::new(module))
    }
}
