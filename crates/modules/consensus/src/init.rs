use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use bfte_consensus_core::bincode::STD_BINCODE_CONFIG;
use bfte_consensus_core::citem::{ICitem, ModuleDyn};
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::module::ModuleKind;
use bfte_consensus_core::ver::{ConsensusVersionMajor, ConsensusVersionMinor};
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::{IModule, ModuleInit, ModuleInitArgs, ModuleInitResult};

use crate::{CURRENT_VERSION, ConsensuseModuleParams, KIND};

pub struct ConsensusModuleInit;

impl ConsensusModuleInit {
    /// [`ModuleConfig`] to use for this module when initializing new consensus
    pub fn init_consensus(&self, consensus_params: ConsensusParams) -> ModuleConfig {
        ModuleConfig {
            kind: KIND,
            version: CURRENT_VERSION,
            config: bincode::encode_to_vec(
                &ConsensuseModuleParams { consensus_params },
                STD_BINCODE_CONFIG,
            )
            .expect("Can't fail")
            .into(),
        }
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
        _args: ModuleInitArgs,
    ) -> ModuleInitResult<Arc<dyn IModule + Send + Sync + 'static>> {
        todo!()
    }

    // DELME
    async fn poll(&self) -> Vec<ModuleDyn<dyn ICitem>> {
        todo!()
    }
}
