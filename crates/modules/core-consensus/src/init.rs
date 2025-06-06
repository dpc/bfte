use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use bfte_consensus_core::bincode::CONSENSUS_BINCODE_CONFIG;
use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_consensus_core::peer_set::PeerSet;
use bfte_consensus_core::ver::{ConsensusVersionMajor, ConsensusVersionMinor};
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::db::{DbResult, ModuleWriteTransactionCtx};
use bfte_module::module::{
    IModule, ModuleInit, ModuleInitArgs, ModuleInitResult, UnsupportedVersionSnafu,
};
use snafu::ensure;
use tokio::sync::watch;

use crate::tables::{self, modules_configs};
use crate::{CURRENT_VERSION, KIND};

pub struct CoreConsensusModuleInit;

impl CoreConsensusModuleInit {
    /// Initialize consensus module
    ///
    /// Since consensus module is the one storing consensus configs for itself
    /// and other modules, it needs to be initialized manually.
    ///
    /// Its own [`ModuleConfig`] it was initialized with
    pub fn bootstrap_consensus(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        module_id: ModuleId,
        peer_set: PeerSet,
    ) -> DbResult<ModuleConfig> {
        let config = ModuleConfig {
            kind: KIND,
            version: CURRENT_VERSION,
            params: bincode::encode_to_vec(
                &super::params::CoreConsensusModuleParams {},
                CONSENSUS_BINCODE_CONFIG,
            )
            .expect("Can't fail")
            .into(),
        };

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
}

#[async_trait]
impl ModuleInit for CoreConsensusModuleInit {
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

        let (propose_citems_tx, propose_citems_rx) = watch::channel(Vec::new());

        args.db
            .write_with_expect(super::CoreConsensusModule::init_db_tx)
            .await;

        Ok(Arc::new(super::CoreConsensusModule {
            db: args.db,
            version: args.module_consensus_version,
            peer_pubkey: args.peer_pubkey,
            propose_citems_rx,
            propose_citems_tx,
        }))
    }
}
