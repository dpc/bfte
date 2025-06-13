use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use bfte_consensus_core::module::ModuleKind;
use bfte_module::module::{
    IModule, IModuleInit, ModuleInitArgs, ModuleInitResult, ModuleSupportedConsensusVersions,
};

use crate::module::MetaModule;
use crate::{CURRENT_VERSION_MAJOR, CURRENT_VERSION_MINOR, KIND};

pub struct MetaModuleInit;

impl MetaModuleInit {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MetaModuleInit {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IModuleInit for MetaModuleInit {
    fn kind(&self) -> ModuleKind {
        KIND
    }

    fn singleton(&self) -> bool {
        true
    }

    fn display_name(&self) -> &'static str {
        "Meta"
    }

    fn supported_versions(&self) -> ModuleSupportedConsensusVersions {
        let mut versions = BTreeMap::new();
        versions.insert(CURRENT_VERSION_MAJOR, CURRENT_VERSION_MINOR);
        versions
    }

    async fn init(
        &self,
        args: ModuleInitArgs,
    ) -> ModuleInitResult<Arc<dyn IModule + Send + Sync + 'static>> {
        // Validate version compatibility
        let supported_version = bfte_consensus_core::ver::ConsensusVersion::new(
            CURRENT_VERSION_MAJOR,
            CURRENT_VERSION_MINOR,
        );
        if args.module_consensus_version != supported_version {
            return Err(bfte_module::module::ModuleInitError::UnsupportedVersion {
                requested: args.module_consensus_version,
                supported: supported_version,
            });
        }

        args.db
            .write_with_expect(|dbtx| MetaModule::init_db_tx(dbtx, args.module_consensus_version))
            .await;

        let module = MetaModule::new(args.module_consensus_version, args.db, args.peer_pubkey);

        Ok(Arc::new(module))
    }
}
