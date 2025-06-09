use std::sync::Arc;

use bfte_consensus_core::bincode::CONSENSUS_BINCODE_CONFIG;
use bfte_consensus_core::citem::CItemRaw;
use bfte_util_bincode::decode_whole;
use bfte_util_error::WhateverResult;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use snafu::ResultExt as _;

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub enum MetaCitem {
    /// A peer votes for a specific value for a given key
    VoteKeyValue { key: u8, value: Arc<[u8]> },
}

impl MetaCitem {
    pub fn encode_to_raw(&self) -> CItemRaw {
        let serialized = bincode::encode_to_vec(self, CONSENSUS_BINCODE_CONFIG)
            .expect("encoding should not fail");
        CItemRaw(serialized.into())
    }

    pub fn decode_from_raw(citem_raw: &CItemRaw) -> WhateverResult<Self> {
        decode_whole(citem_raw, CONSENSUS_BINCODE_CONFIG)
            .whatever_context("Failed to decode CoreConsensusCitem")
    }
}
