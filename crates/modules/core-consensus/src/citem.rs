use bfte_consensus_core::bincode::CONSENSUS_BINCODE_CONFIG;
use bfte_consensus_core::citem::CItemRaw;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_util_error::WhateverResult;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use snafu::whatever;

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub enum CoreConsensusCitem {
    VoteAddPeer(PeerPubkey),
    VoteRemovePeer(PeerPubkey),
}

impl CoreConsensusCitem {
    pub fn to_citem_raw(&self) -> CItemRaw {
        let serialized = bincode::encode_to_vec(self, CONSENSUS_BINCODE_CONFIG)
            .expect("encoding should not fail");
        CItemRaw(serialized.into())
    }

    pub fn from_citem_raw(citem_raw: &CItemRaw) -> WhateverResult<Self> {
        match bincode::decode_from_slice(citem_raw, CONSENSUS_BINCODE_CONFIG) {
            Ok((citem, _)) => Ok(citem),
            Err(e) => whatever!("Failed to decode CoreConsensusCitem: {e}"),
        }
    }
}
