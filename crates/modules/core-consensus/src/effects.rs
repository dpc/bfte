use bfte_consensus_core::module::ModuleKind;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_module::effect::{EffectId, EffectKind};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub struct AddPeerEffect {
    pub peer: PeerPubkey,
}

impl EffectKind for AddPeerEffect {
    const MODULE_KIND: ModuleKind = crate::KIND;
    const EFFECT_ID: EffectId = EffectId::new(0);
}

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub enum CoreConsensusCItemEffect {
    AddPeer(PeerPubkey),
}
