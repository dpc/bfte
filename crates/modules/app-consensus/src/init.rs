use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use bfte_consensus_core::bincode::CONSENSUS_BINCODE_CONFIG;
use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_consensus_core::peer_set::PeerSet;
use bfte_consensus_core::ver::{ConsensusVersion, ConsensusVersionMajor, ConsensusVersionMinor};
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::db::{DbResult, ModuleDatabase, ModuleWriteTransactionCtx};
use bfte_module::module::{
    IModule, IModuleInit, ModuleInitArgs, ModuleInitResult, UnsupportedVersionSnafu,
};
use snafu::ensure;
use tokio::sync::watch;

use super::AppConsensusModule;
use crate::tables::{self, modules_configs};
use crate::{CURRENT_VERSION_MAJOR, CURRENT_VERSION_MINOR, KIND};

pub struct AppConsensusModuleInit;

impl AppConsensusModuleInit {
    /// Initialize consensus module
    ///
    /// Since consensus module is the one storing consensus configs for itself
    /// and other modules, it needs to be initialized manually on first run.
    pub fn bootstrap_consensus(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        module_id: ModuleId,
        peer_set: PeerSet,
    ) -> DbResult<ModuleConfig> {
        let version = ConsensusVersion::new(CURRENT_VERSION_MAJOR, CURRENT_VERSION_MINOR);
        let config = ModuleConfig {
            kind: KIND,
            version,
            params: bincode::encode_to_vec(
                &super::params::CoreConsensusModuleParams {},
                CONSENSUS_BINCODE_CONFIG,
            )
            .expect("Can't fail")
            .into(),
        };

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

    /// Get modules configs without creating an instance of `AppConsensus`
    /// itself
    ///
    /// This is useful on start, as `node-app` can't create an instance of
    /// `AppConsensus` without knowing its config first.
    pub async fn get_modules_configs(
        &self,
        db: &ModuleDatabase,
    ) -> BTreeMap<ModuleId, ModuleConfig> {
        AppConsensusModule::get_module_configs_static(db).await
    }
}

#[async_trait]
impl IModuleInit for AppConsensusModuleInit {
    fn kind(&self) -> ModuleKind {
        crate::KIND
    }

    fn display_name(&self) -> &'static str {
        "App Consensus"
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
            .write_with_expect(AppConsensusModule::init_db_tx)
            .await;

        let module = AppConsensusModule {
            db: args.db,
            version: args.module_consensus_version,
            peer_pubkey: args.peer_pubkey,
            propose_citems_rx,
            propose_citems_tx,
        };

        module.refresh_consensus_proposals().await;

        Ok(Arc::new(module))
    }
}
