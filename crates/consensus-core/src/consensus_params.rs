use std::sync::Arc;

use bfte_util_array_type::{
    array_type_define, array_type_fixed_size_define, array_type_impl_base32_str,
    array_type_impl_debug_as_display, array_type_impl_serde, array_type_impl_zero_default,
};
use bfte_util_bincode::decode_whole;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt as _, Snafu};

use crate::bincode::CONSENSUS_BINCODE_CONFIG;
use crate::block::{BlockHash, BlockRound};
use crate::framed_payload_define;
use crate::num_peers::{NumPeers, ToNumPeers as _};
use crate::peer::{PeerIdx, PeerPubkey};
use crate::peer_set::PeerSet;
use crate::signed::Hashable;
use crate::timestamp::Timestamp;
use crate::ver::ConsensusVersion;

array_type_fixed_size_define! {
    /// Length of block payload
    ///
    /// This is committed in the header, mostly so it's possible
    /// to propagate payloads via BAO incremental verification.
    #[derive(Encode, Decode, Clone, Copy, Serialize, Deserialize)]
    pub struct ConsensusParamsLen(u32);
}

framed_payload_define! {
    pub struct ConsensusParamsRaw;

    ConsensusParamsHash;
    ConsensusParamsLen;

    pub struct ConsensusParamsSlice;

}
/// Core consensus parameters
///
/// In each round peers always know the rules of the consensus,
/// and that information is being committed to in every block
/// to allow other nodes to easily verify it even when they
/// don't (yet, or at all) track the consensus state themselves.
#[derive(Decode, Encode, Clone, PartialEq, Eq, Debug)]
pub struct ConsensusParams {
    /// Version of this [`ConsensusParams`] format
    pub consensus_params_format_version: u8,

    /// A federation starts with a single module - core consensus module,
    /// and this field records its starting module consensus version.
    ///
    /// Just in case, this should be read from [`Self`] committed to
    /// in the round 0.
    pub init_core_module_cons_version: ConsensusVersion,

    /// The timestamp the federation was created at
    ///
    /// For initial [`ConsensusParams`] this is the consensus/federation
    /// creation time.
    ///
    /// For following [`ConsensusParams`] this is the timestamp from the block
    /// at which the consensus params was scheduled (but not yet applied).
    pub timestamp: Timestamp,

    /// [`BlockRound`] this consensus parameters was/will be scheduled.
    ///
    /// As voting on consensus changes is performed, the peers deterministically
    /// reach the decision about new consensus parameters at the same round.
    ///
    /// Given an amount of peers at that round, a certain delay is added, to
    /// ensure some time for all peers to reach a finality and add it to
    /// their consensus params schedule.
    pub scheduled_round: BlockRound,

    /// [`BlockRound`] this consensus parameters was/will be applied.
    pub applied_round: BlockRound,

    /// Block round and hash of some (potentially distant) historical notarized
    /// block.
    ///
    /// When joining the consensus there's a problem that there's nothing to
    /// stop the peer(s) that initially started the Federation from writing
    /// an alternative history from the start, and attempt to fool a node
    /// joining the consensus into following it. Given a current state of
    /// the Federation, the new node would have to rewind the history one by
    /// one backwards first, just to trustlessly start applying the block
    /// one by one from start.
    ///
    /// To help with that anytime a [`Self`] is created, a previous notarized
    /// block is being committed to here (most recent one before round
    /// `applied_round / 2`). This allows any node to trustlessly rewind the
    /// chain from the newest to the oldest block in `O(log(N))`.
    pub prev_mid_block: Option<(BlockRound, BlockHash)>,

    /// Set of voting peers
    pub peers: PeerSet,
}

impl ConsensusParams {
    pub const FORMAT_VERSION: u8 = 0;
    /// New, empty, zeroed, possibly nonsensical consensus params
    ///
    /// Useful mostly for testing.
    pub fn new_test_dummy() -> Self {
        Self {
            peers: PeerSet::new(),
            prev_mid_block: None,
            consensus_params_format_version: 0,
            init_core_module_cons_version: ConsensusVersion::new(0, 0),
            timestamp: Timestamp::ZERO,
            scheduled_round: 0.into(),
            applied_round: 0.into(),
        }
    }

    pub fn make_change(
        self,
        peer_set: PeerSet,
        prev_mid_block: Option<(BlockRound, BlockHash)>,
        scheduled_round: BlockRound,
    ) -> Self {
        let applied_round = scheduled_round
            .checked_add(self.consensus_params_schedulign_delay())
            .expect("Can't ran out of u64 of rounds");
        Self {
            peers: peer_set,
            timestamp: Timestamp::now(),
            prev_mid_block,
            scheduled_round,
            applied_round,
            ..self
        }
    }

    pub fn consensus_params_schedulign_delay(&self) -> u64 {
        u64::try_from(self.peers.to_num_peers().total()).expect("Can't fail") + 32
    }

    pub fn num_peers(&self) -> NumPeers {
        self.peers.to_num_peers()
    }

    pub fn leader_idx(&self, round: BlockRound) -> PeerIdx {
        round.leader_idx(self.num_peers())
    }

    pub fn hash(&self) -> ConsensusParamsHash {
        Hashable::hash(self).into()
    }

    pub fn len(&self) -> ConsensusParamsLen {
        self.to_raw().len()
    }

    pub fn hash_and_len(&self) -> (ConsensusParamsHash, ConsensusParamsLen) {
        let raw = self.to_raw();
        let hash = raw.hash();
        let len = raw.len();
        debug_assert_eq!(hash, self.hash());
        (hash, len)
    }

    pub fn find_peer_idx(&self, peer_pubkey: PeerPubkey) -> Option<PeerIdx> {
        self.peers
            .iter()
            .enumerate()
            .find(|(_i, p)| **p == peer_pubkey)
            .map(|(i, _)| PeerIdx::from(u8::try_from(i).expect("Can't overflow")))
    }

    pub fn to_raw(&self) -> ConsensusParamsRaw {
        ConsensusParamsRaw(
            bincode::encode_to_vec(self, CONSENSUS_BINCODE_CONFIG)
                .expect("Can't fail")
                .into(),
        )
    }

    pub fn from_raw(raw: &ConsensusParamsRaw) -> ConsensusParamsDecodeResult<Self> {
        let decoded: ConsensusParams =
            decode_whole(&raw.0, CONSENSUS_BINCODE_CONFIG).context(BincodeSnafu)?;

        if decoded.consensus_params_format_version != 0 {
            return MismatchedFormatVersionSnafu {
                version: decoded.consensus_params_format_version,
            }
            .fail();
        }

        Ok(decoded)
    }

    pub fn iter_peers(&self) -> impl Iterator<Item = (PeerIdx, PeerPubkey)> {
        self.peers.iter().enumerate().map(|(i, peer_pubkey)| {
            (
                PeerIdx::from(u8::try_from(i).expect("Can't fail")),
                *peer_pubkey,
            )
        })
    }
}

impl Hashable for ConsensusParams {}

#[derive(Snafu, Debug)]
pub enum ConsensusParamsDecodeError {
    Bincode { source: bincode::error::DecodeError },
    MismatchedFormatVersion { version: u8 },
    UnknownVersion { version: ConsensusVersion },
}

pub type ConsensusParamsDecodeResult<T> = Result<T, ConsensusParamsDecodeError>;

array_type_define! {
    #[derive(Encode, Decode, Copy, Clone)]
    pub struct ConsensusParamsHash[32];
}

array_type_impl_zero_default!(ConsensusParamsHash);
array_type_impl_base32_str!(ConsensusParamsHash);
array_type_impl_serde!(ConsensusParamsHash);
array_type_impl_debug_as_display!(ConsensusParamsHash);

impl From<blake3::Hash> for ConsensusParamsHash {
    fn from(value: blake3::Hash) -> Self {
        Self(*value.as_bytes())
    }
}
