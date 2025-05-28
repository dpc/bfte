// SPDX-License-Identifier: MIT

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Arc;

use bfte_consensus_core::module::config::ModuleConfigHash;
use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_consensus_core::ver::ConsensusVersion;
use bfte_module_core::module::{DynModuleInit, Module, ModuleInit};
use bfte_node_app_core::NodeAppApi;
use bfte_util_error::WhateverResult;

pub struct NodeApp {
    modules_inits: BTreeMap<ModuleKind, Arc<dyn ModuleInit + Send + Sync>>,
    modules: BTreeMap<ModuleId, Arc<dyn Module + Send + Sync>>,
    modules_config_hashes: BTreeMap<ModuleId, (ConsensusVersion, ModuleConfigHash)>,
}

pub async fn run(
    _node_api: NodeAppApi,
    _module_inits: BTreeMap<ModuleKind, DynModuleInit>,
) -> WhateverResult<Infallible> {
    todo!()
}
