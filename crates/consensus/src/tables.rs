use bfte_consensus_core::Signature;
use bfte_consensus_core::block::{BlockHash, BlockHeader, BlockPayloadRaw, BlockRound};
use bfte_consensus_core::consensus_params::{ConsensusParams, ConsensusParamsHash};
use bfte_consensus_core::peer::{PeerIdx, PeerPubkey};
use bfte_consensus_core::vote::SignedVote;
use bfte_util_db::def_table;

def_table! {
    /// Tracks consensus database/schema version
    db_version: () => u64
}

def_table! {
    /// Current consensus round we're in
    ///
    /// All blocks (if present) in rounds less than this value are notarized
    /// (have enough votes).
    cons_current_round: () => BlockRound
}

def_table! {
    /// Current round we want to finalize
    ///
    /// NOTE: Unlike in the original paper this is open-ended bound,
    /// so `N` means all the rounds *less* than `N` are finalized, but
    /// not `N`. This is so that `0` means "no blocks finalized yet".
    cons_finality_consensus: () => BlockRound
}

def_table! {
    /// Current count of notarized rounds, as reported by a given peer
    ///
    /// Since peers can change between rounds we track them by pubkey.
    ///
    /// Once `threshold` of peers for the current [`cons_finality_threshold`],
    ///
    /// NOTE: Unlike in the original paper this is open-ended bound,
    /// so `N` means all the rounds *less* than `N` are finalized, but
    /// not `N`. This is so that `0` means "no blocks finalized yet".
    cons_finality_votes: PeerPubkey => BlockRound
}

def_table! {
    /// [`ConsensusParams`] schedule
    ///
    /// Consensus parameters for a given block round is the
    /// first entry here with a block round equal or less
    /// than this round.
    ///
    /// Must be initialized with params for round 0
    cons_params_schedule: BlockRound => ConsensusParamsHash
}

def_table! {
    /// [`ConsensusParams`] schedule
    ///
    /// Consensus parameters for a given block round is the
    /// first entry here with a block round equal or less
    /// than this round.
    ///
    /// Must be initialized with params for round 0
    cons_params: ConsensusParamsHash => ConsensusParams
}

def_table! {
    /// Blocks proposals (only non-dummy)
    ///
    /// Any time we get a valid and non-dummy proposal, we write it here, or reject
    /// the proposal if something is here already. This guarantees we never sign
    /// two different proposals for the same round.
    ///
    /// Cleanup: table can be pruned after round is finalized.
    cons_blocks_proposals: BlockRound => BlockHeader
}

def_table! {
    /// Blocks, notarized
    ///
    /// Note: dummy blocks are not stored here at all, and are implied.
    ///
    /// For any block here we always need to have a `threshold` number
    /// of valid votes in `cons_votes_blocks`
    cons_blocks_notarized: BlockRound => BlockHeader
}

def_table! {
    /// Block pins
    ///
    /// To ensure that a chain being synced is the one that was
    /// intended, higher level might insert pinned hashes.
    ///
    /// This will cause the consensus code to reject blocks other than
    /// the pinned one at a given block round, protecting from
    /// histories that do not match etc.
    cons_blocks_pinned: BlockRound => BlockHash
}

def_table! {
    /// Payloads corresponding to blocks
    ///
    /// Main reason for using a different table is is to avoid fetching big
    /// payload when only checking stuff about the header.
    ///
    /// Another one is being able to re-use it for both `cons_blocks_notarized`
    /// and `cons_blocks_proposals`. It's unnecessary to every garbage collect this
    /// table.
    cons_blocks_payloads: bfte_consensus_core::block::BlockPayloadHash => BlockPayloadRaw
}

def_table! {
    /// Votes for a dummy block in a [`BlockRound`] by a [`PeerIdx`]
    ///
    /// If there's a corresponding notarized block, there must be
    /// here `threshold` of valid votes for it, and only valid ones.
    ///
    /// If there isn't, but there's a proposal, only valid votes for it
    /// should be here.
    ///
    /// If there isn't even a proposal, votes are valid or not, and
    /// their number doesn't mean much until we can verify it.
    cons_votes_block: (BlockRound, PeerIdx)  => SignedVote
}

def_table! {
    /// Votes for a dummy block in a [`BlockRound`] by a [`PeerIdx`]
    ///
    /// Note: we don't need to have all dummy votes, to consider
    /// dummy vote as notarized, we're find skipping. Because of
    /// this when someone asks for our vote for a given round,
    /// and we don't find it, we might need to produce it on the
    /// flight, or just not respond and rely on notarization block
    /// responses to have the peer switch.
    ///
    /// Votes for dummy blocks can be deleted when the round or
    /// any round above is finalized. No one cares at that point.
    cons_votes_dummy: (BlockRound, PeerIdx)  => Signature
}
