use std::collections::BTreeMap;
use std::sync::Arc;

use bfte_util_array_type::{
    array_type_define, array_type_fixed_size_define, array_type_fixed_size_impl_serde,
    array_type_impl_base32_str, array_type_impl_debug_as_display, array_type_impl_serde,
    array_type_impl_zero_default,
};
use bincode::{Decode, Encode};
use num_bigint::BigUint;
use serde::Deserialize;
use snafu::Snafu;

use crate::consensus_params::{ConsensusParams, ConsensusParamsHash, ConsensusParamsLen};
use crate::framed_payload_define;
use crate::num_peers::NumPeers;
use crate::peer::PeerIdx;
use crate::signed::{Hashable, Signable};
use crate::ver::ConsensusVersion;

array_type_fixed_size_define! {
    /// Non-dumy block sequence number
    ///
    /// [`BlockSeq`] is incremented in each non-dummy block.
    ///
    /// It's fixed sized encoded, to make the size of the [`BlockHeader`] constant.
    #[derive(Encode, Decode, Clone, Copy)]
    pub struct BlockSeq(u64);
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
}

impl Hashable for BlockRound {}

// Just some simple test vectors to ensure the leader index does change by
// accident
#[test]
fn block_round_leader_test() {
    for (n, r, leader_fixture) in [
        (1u8, 0, 0),
        (15, 0, 1),
        (15, 1, 7),
        (15, 2, 10),
        (10, 0, 1),
        (10, 1, 2),
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
    padding: [u8; 4],                        // 4B
    pub consensus_version: ConsensusVersion, // 4B
    pub seq: BlockSeq,                       // 8B
    pub round: BlockRound,                   // 8B

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
pub enum ContentMismatchError {
    PayloadHashMismatch,
    PayloadLenMismatch,
    ConsensusHashMismatch,
    ConsensusLenMismatch,
    ConsensusVersionMismatch,
}

pub type ContentMismatchResult<T> = std::result::Result<T, ContentMismatchError>;

impl Hashable for BlockHeader {}
impl Signable for BlockHeader {
    const TAG: [u8; 4] = *b"blhd";
}

#[bon::bon]
impl BlockHeader {
    #[builder]
    pub fn new(
        prev: Option<BlockHeader>,
        round: BlockRound,
        consensus_params: &ConsensusParams,
        payload: &BlockPayloadRaw,
    ) -> Self {
        Self {
            consensus_version: consensus_params.version,
            seq: prev
                .map(|p| p.seq.next().expect("Can't ran out"))
                .unwrap_or_default(),
            round,
            padding: [0u8; 4],
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

    pub fn verify_content(
        &self,
        block_round_consensus_params_hash: ConsensusParamsHash,
        block_round_consensus_params_len: ConsensusParamsLen,
        consensus_version: ConsensusVersion,
        payload: &BlockPayloadRaw,
    ) -> ContentMismatchResult<()> {
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

        if self.consensus_params_hash != block_round_consensus_params_hash {
            ConsensusHashMismatchSnafu.fail()?;
        }
        if self.consensus_params_len != block_round_consensus_params_len {
            ConsensusLenMismatchSnafu.fail()?;
        }
        if self.consensus_version != consensus_version {
            ConsensusVersionMismatchSnafu.fail()?;
        }
        Ok(())
    }

    pub fn new_dummy(round: BlockRound, consensus_params: &ConsensusParams) -> Self {
        let (consensus_params_hash, consensus_params_len) = consensus_params.hash_and_len();
        Self {
            consensus_version: consensus_params.version,
            padding: [0u8; 4],
            seq: BlockSeq::default(),
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
            prev_block.seq.next() == Some(self.seq)
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

array_type_define! {
    #[derive(Encode, Decode)]
    pub struct BlockSignature[32];
}
array_type_impl_zero_default!(BlockSignature);
array_type_impl_base32_str!(BlockSignature);
array_type_impl_serde!(BlockSignature);

#[cfg(test)]
mod tests;
