use bfte_consensus_core::module::ModuleKind;
use bfte_consensus_core::module::config::ModuleParamsRaw;
use bfte_consensus_core::ver::ConsensusVersion;
use bincode::{Decode, Encode};

#[derive(PartialEq, Eq, Encode, Decode)]
pub struct ModuleConfig {
    pub kind: ModuleKind,
    pub version: ConsensusVersion,
    pub params: ModuleParamsRaw,
}
