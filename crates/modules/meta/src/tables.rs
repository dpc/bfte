use std::sync::Arc;

use bfte_consensus_core::peer::PeerPubkey;
use bfte_util_db::def_table;

def_table! {
    /// Tracks votes for key-value pairs
    /// (key, voter) -> value they voted for
    key_value_votes: (u8, PeerPubkey) => Arc<[u8]>
}

def_table! {
    /// Tracks the current agreed consensus values for keys
    /// key -> agreed value
    consensus_values: u8 => Arc<[u8]>
}

def_table! {
    /// Tracks pending key-value proposals that this node wants to submit
    /// key -> value to propose
    pending_proposals: u8 => Arc<[u8]>
}
