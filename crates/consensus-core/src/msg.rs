use bincode::{Decode, Encode};
use derive_more::From;

use crate::block::{BlockHeader, BlockPayloadRaw, BlockRound};
use crate::signed::{Hashable, Notarized, Signable, Signed};

/// Wait for the peer vote on the block in the round
#[derive(Decode, Encode, Clone, Copy, PartialEq, Eq)]
pub struct WaitVoteRequest {
    pub round: BlockRound,
    /// The requester already has a non-dummy vote, and is
    /// only interested in timeout dummy vote.
    pub only_dummy: bool,
}

#[derive(Decode, Encode, Clone, Debug)]
pub enum WaitVoteResponse {
    /// If the peer is a leader it should respond with its proposal
    Proposal {
        block: Signed<BlockHeader>,
        payload: BlockPayloadRaw,
    },
    /// If not, it should vote on the proposal it received or dummy block
    Vote { block: Signed<BlockHeader> },
}

impl WaitVoteResponse {
    pub fn block(&self) -> &Signed<BlockHeader> {
        match self {
            WaitVoteResponse::Proposal { block, payload: _ } => block,
            WaitVoteResponse::Vote { block } => block,
        }
    }

    pub fn is_proposal(&self) -> bool {
        matches!(self, WaitVoteResponse::Proposal { .. })
    }
}

/// Wait for the first non-dummy notarized block in range
/// `min_notarized_round..` or notarized block (dummy or not) exactly at
/// `cur_round`.
///
/// When requester makes this call it means that the last non-dummy
/// notarized block it has is at `min_notarized_round - 1` (if not `0`),
/// and the current round is `cur_round`.
///
/// The respondent should look for a first notarized non-dummy block in the
/// range `min_notarized_round..`, and if not available for any
/// notarized block `cur_round`. If neither is available, it should block and
/// wait until one is available.
///
/// This rpc allows requester to quickly enter next round and/or
/// switch to any competing notarized blocks that it was not aware of.
#[derive(Decode, Encode, Clone, Copy, PartialEq, Eq)]
pub struct WaitNotarizedBlockRequest {
    /// Highest round at (and past) which requester does not have any notarized
    /// non-dummy block yet and is interested in them.
    pub min_notarized_round: BlockRound,
    /// Current round at which the requester is, and is interested in a
    /// notarized block, even if dummy.
    pub cur_round: BlockRound,
}

#[derive(Decode, Encode, Clone, Debug)]
pub struct WaitNotarizedBlockResponse {
    pub block: Notarized<BlockHeader>,
    pub payload: BlockPayloadRaw,
}

/// Wait for the first unnotarized non-dummy block round on the peer to change
///
/// `prev` contains previous known value
#[derive(Decode, Encode, Clone, Copy, PartialEq, Eq)]
pub struct WaitFinalityVoteRequest {
    pub round: BlockRound,
}

/// Response to [`WaitFinalityVoteUpdateRequest`]
///
/// `round` with an update (higher than `prev` in the request)
#[derive(Decode, Encode, Clone)]
pub struct WaitFinalityVoteResponse {
    pub update: Signed<FinalityVoteUpdate>,
}

#[derive(Clone, Encode, Decode, Debug, From)]
pub struct FinalityVoteUpdate(pub BlockRound);

impl FinalityVoteUpdate {
    pub fn new(round: BlockRound) -> Self {
        Self(round)
    }
}

impl Hashable for FinalityVoteUpdate {
    const TAG: [u8; 4] = *b"furu";
}
impl Signable for FinalityVoteUpdate {}
