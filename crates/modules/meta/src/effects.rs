use std::sync::Arc;

use bfte_consensus_core::module::ModuleKind;
use bfte_module::effect::{EffectId, EffectKind};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub struct KeyValueConsensusEffect {
    pub key: u8,
    pub value: Arc<[u8]>,
}

impl EffectKind for KeyValueConsensusEffect {
    const MODULE_KIND: ModuleKind = crate::KIND;
    const EFFECT_ID: EffectId = EffectId::new(0);
}
