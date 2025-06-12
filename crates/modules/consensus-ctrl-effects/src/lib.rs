use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_consensus_core::peer::PeerPubkey;
use bfte_consensus_core::peer_set::PeerSet;
use bfte_consensus_core::ver::ConsensusVersion;
use bfte_module::effect::{EffectId, EffectKind};
use bfte_module::kinds::MODULE_KIND_CONSENSUS_CTRL;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

pub const KIND: ModuleKind = MODULE_KIND_CONSENSUS_CTRL;

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub struct AddPeerEffect {
    pub peer: PeerPubkey,
}

impl EffectKind for AddPeerEffect {
    const MODULE_KIND: ModuleKind = KIND;
    const EFFECT_ID: EffectId = EffectId::new(0);
}

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub struct RemovePeerEffect {
    pub peer: PeerPubkey,
}

impl EffectKind for RemovePeerEffect {
    const MODULE_KIND: ModuleKind = KIND;
    const EFFECT_ID: EffectId = EffectId::new(1);
}

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub struct ConsensusParamsChange {
    pub peer_set: PeerSet,
}

impl EffectKind for ConsensusParamsChange {
    const MODULE_KIND: ModuleKind = KIND;
    const EFFECT_ID: EffectId = EffectId::new(2);
}

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub struct ModuleVersionUpgradeEffect {
    pub module_id: ModuleId,
    pub old_version: ConsensusVersion,
    pub new_version: ConsensusVersion,
}

impl EffectKind for ModuleVersionUpgradeEffect {
    const MODULE_KIND: ModuleKind = KIND;
    const EFFECT_ID: EffectId = EffectId::new(3);
}

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub struct AddModuleEffect {
    pub module_kind: ModuleKind,
    pub module_id: ModuleId,
    pub consensus_version: ConsensusVersion,
}

impl EffectKind for AddModuleEffect {
    const MODULE_KIND: ModuleKind = KIND;
    const EFFECT_ID: EffectId = EffectId::new(4);
}
