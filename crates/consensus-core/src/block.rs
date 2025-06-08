use std::collections::BTreeMap;
use std::sync::Arc;

use bfte_util_array_type::{
    array_type_define, array_type_fixed_size_define, array_type_fixed_size_impl_serde,
    array_type_impl_base32_str, array_type_impl_debug_as_display, array_type_impl_serde,
    array_type_impl_zero_default,
};
use bfte_util_bincode::decode_whole;
use bfte_util_error::WhateverResult;
use bincode::{Decode, Encode};
use num_bigint::BigUint;
use serde::Deserialize;
use snafu::{ResultExt as _, Snafu};

use crate::bincode::CONSENSUS_BINCODE_CONFIG;
use crate::citem::CItem;
use crate::consensus_params::{ConsensusParams, ConsensusParamsHash, ConsensusParamsLen};
use crate::framed_payload_define;
use crate::num_peers::NumPeers;
use crate::peer::PeerIdx;
use crate::signed::{Hashable, Signable};
use crate::timestamp::Timestamp;

array_type_fixed_size_define! {
    /// Non-dumy block sequence number
    ///
    /// [`BlockSeq`] is incremented in each non-dummy block.
    ///
    /// It's fixed sized encoded, to make the size of the [`BlockHeader`] constant.
    #[derive(Encode, Decode, Clone, Copy)]
    pub struct BlockSeq(u32);
}

array_type_fixed_size_define! {
    /// Round the block was produced in
    #[derive(Encode, Decode, Clone, Copy)]
    pub struct BlockRound(u64);
}
array_type_fixed_size_impl_serde!(BlockRound);

impl BlockRound {
    pub fn leader_idx(self, n: NumPeers) -> PeerIdx {
        let bytes = (BigUint::from_bytes_be(self.hash().as_bytes()) % BigUint::from(n.total()))
            .to_bytes_be();

        debug_assert_eq!(bytes.len(), 1);

        let idx = bytes[0];
        debug_assert!(usize::from(idx) < n.total());

        idx.into()
    }

    pub fn half(self) -> Self {
        Self::from(self.to_number() / 2)
    }
}

impl Hashable for BlockRound {}

// Just some simple test vectors to ensure the leader index does change by
// accident
#[test]
fn block_round_leader_test() {
    for (n, r, leader_fixture) in [
        (1u8, 0, 0),
        (15, 0, 12),
        (15, 1, 13),
        (15, 2, 7),
        (10, 0, 7),
        (10, 1, 3),
    ] {
        let leader_now = BlockRound::from(r).leader_idx(NumPeers::from(n));
        assert_eq!(
            PeerIdx::from(leader_fixture),
            leader_now,
            "{n} {r} {leader_fixture} -> {n} {r} {leader_now}"
        );
    }
}
array_type_define! {
    #[derive(Encode, Decode, Copy, Clone)]
    pub struct BlockHash[32];
}
array_type_impl_zero_default!(BlockHash);
array_type_impl_base32_str!(BlockHash);
array_type_impl_serde!(BlockHash);
array_type_impl_debug_as_display!(BlockHash);

impl From<blake3::Hash> for BlockHash {
    fn from(value: blake3::Hash) -> Self {
        Self(*value.as_bytes())
    }
}

impl From<BlockHash> for blake3::Hash {
    fn from(value: BlockHash) -> Self {
        blake3::Hash::from_bytes(value.0)
    }
}

#[derive(Debug, Encode, Decode, Copy, Clone, PartialEq, Eq)]
pub struct BlockHeader {
    /// Version of this header format
    ///
    /// Should be `0`, and not expected to need to change, but reserved just in
    /// case.
    pub header_version: u8, // 1B
    /// Just to align things, could be used for non-consensus flags
    /// in the future (e.g. round leader signaling certain networking
    /// conditions, etc.)
    padding: [u8; 3], // 3B

    pub seq: BlockSeq,        // 8B
    pub round: BlockRound,    // 8B
    pub timestamp: Timestamp, // 8B

    /// Commits to [`BlockPayload`]'s length
    ///
    /// Notably both length and hash are committed to, to allow
    /// BAO incrementally verified transfers.
    pub payload_len: BlockPayloadLen, // 4B

    /// Commits to [`ConsensusParams`]'s length
    ///
    /// Notably both length and hash are committed to, to allow
    /// BAO incrementally verified transfers.
    pub consensus_params_len: ConsensusParamsLen, // 4B

    /// Commits to previous non-dummy `BlockHeader`
    pub prev_block_hash: BlockHash, // 32B

    /// Commits to [`ConsensusParams`] used for this block
    pub consensus_params_hash: ConsensusParamsHash, // 32B

    /// Commits to [`BlockPayload`]
    pub payload_hash: BlockPayloadHash, // 32B
}

#[derive(Debug, Snafu)]
pub enum VerifyWithContentError {
    PayloadHashMismatch,
    PayloadLenMismatch,
    ConsensusHashMismatch,
    UnknownVersion,
    ConsensusLenMismatch,
    ConsensusVersionMismatch,
}

pub type VerifyWithContentMismatchResult<T> = std::result::Result<T, VerifyWithContentError>;

impl Hashable for BlockHeader {}
impl Signable for BlockHeader {
    const TAG: [u8; 4] = *b"blhd";
}

#[bon::bon]
impl BlockHeader {
    #[builder]
    pub fn new(
        prev: Option<BlockHeader>,
        timestamp: Timestamp,
        round: BlockRound,
        consensus_params: &ConsensusParams,
        payload: &BlockPayloadRaw,
    ) -> Self {
        Self {
            header_version: 0,
            padding: [0u8; 3],
            timestamp,
            seq: prev.map(|p| p.seq.next_wrapping()).unwrap_or_default(),
            round,
            payload_len: payload.len(),
            prev_block_hash: prev.map(|p| p.hash()).unwrap_or_default(),
            consensus_params_hash: consensus_params.hash(),
            consensus_params_len: consensus_params.len(),
            payload_hash: payload.hash(),
        }
    }
}

impl BlockHeader {
    pub fn hash(&self) -> BlockHash {
        Hashable::hash(self).into()
    }

    pub fn verify_with_content(
        &self,
        block_round_consensus_params_hash: ConsensusParamsHash,
        block_round_consensus_params_len: ConsensusParamsLen,
        payload: &BlockPayloadRaw,
    ) -> VerifyWithContentMismatchResult<()> {
        if self.is_dummy() {
            if payload.len() != 0.into() {
                PayloadLenMismatchSnafu.fail()?;
            }
        } else {
            if payload.hash() != self.payload_hash {
                PayloadHashMismatchSnafu.fail()?;
            }
            if payload.len() != self.payload_len {
                PayloadLenMismatchSnafu.fail()?;
            }
        }

        if self.header_version != 0 {
            UnknownVersionSnafu.fail()?;
        }

        if self.consensus_params_hash != block_round_consensus_params_hash {
            ConsensusHashMismatchSnafu.fail()?;
        }
        if self.consensus_params_len != block_round_consensus_params_len {
            ConsensusLenMismatchSnafu.fail()?;
        }
        Ok(())
    }

    pub fn new_dummy(round: BlockRound, consensus_params: &ConsensusParams) -> Self {
        let (consensus_params_hash, consensus_params_len) = consensus_params.hash_and_len();
        Self {
            header_version: 0,
            padding: [0u8; 3],
            timestamp: Timestamp::ZERO,
            seq: BlockSeq::ZERO,
            round,
            prev_block_hash: BlockHash::ZERO,
            consensus_params_len,
            payload_len: BlockPayloadLen::ZERO,
            consensus_params_hash,
            payload_hash: BlockPayloadHash::ZERO,
        }
    }

    pub fn is_dummy(&self) -> bool {
        self.seq == BlockSeq::ZERO
            && self.prev_block_hash == BlockHash::ZERO
            && self.payload_hash == BlockPayloadHash::ZERO
            && self.payload_len == BlockPayloadLen::ZERO
    }

    pub fn does_directly_extend(&self, prev_notarized_block: Option<BlockHeader>) -> bool {
        if let Some(prev_block) = prev_notarized_block {
            prev_block.seq.next_wrapping() == self.seq
                && prev_block.round < self.round
                && prev_block.hash() == self.prev_block_hash
        } else {
            // Note: multiple first blocks could be dummy blocks, so `round` does
            // not need to be a `0` for the block to commit to zero: .seq & prev_block_hash
            self.seq == BlockSeq::ZERO && self.prev_block_hash == BlockHash::ZERO
        }
    }
}

#[derive(Encode, Decode)]
pub struct SignedBlock {
    block: BlockHeader,
    signatures: BTreeMap<PeerIdx, BlockSignature>,
}

array_type_fixed_size_define! {
    /// Length of block payload
    ///
    /// This is committed in the header, mostly so it's possible
    /// to propagate payloads via BAO incremental verification.
    #[derive(Encode, Decode, Clone, Copy)]
    pub struct BlockPayloadLen(u32);
}

array_type_define! {
    #[derive(Encode, Decode, Copy, Clone)]
    pub struct BlockPayloadHash[32];
}
array_type_impl_zero_default!(BlockPayloadHash);
array_type_impl_base32_str!(BlockPayloadHash);
array_type_impl_serde!(BlockPayloadHash);
array_type_impl_debug_as_display!(BlockPayloadHash);

impl From<blake3::Hash> for BlockPayloadHash {
    fn from(value: blake3::Hash) -> Self {
        Self(*value.as_bytes())
    }
}

framed_payload_define! {
    pub struct BlockPayloadRaw;

    BlockPayloadHash;
    BlockPayloadLen;

    pub struct BlockPayloadSlice;
}

impl BlockPayloadRaw {
    pub fn decode_citems(&self) -> WhateverResult<Arc<[CItem]>> {
        decode_whole(&self.as_inner_slice(), CONSENSUS_BINCODE_CONFIG)
            .whatever_context("Unable to decode block payload")
    }

    pub fn encode_citems(citems: &[CItem]) -> Self {
        Self(
            bincode::encode_to_vec(citems, CONSENSUS_BINCODE_CONFIG)
                .expect("Can't fail")
                .into(),
        )
    }
}
array_type_define! {
    #[derive(Encode, Decode)]
    pub struct BlockSignature[32];
}
array_type_impl_zero_default!(BlockSignature);
array_type_impl_base32_str!(BlockSignature);
array_type_impl_serde!(BlockSignature);

#[cfg(test)]
mod tests;
