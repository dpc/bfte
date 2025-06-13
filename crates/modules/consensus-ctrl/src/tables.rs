use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_consensus_core::peer::PeerPubkey;
use bfte_consensus_core::ver::{ConsensusVersion, ConsensusVersionMinor};
use bfte_module::module::config::ModuleConfig;
use bfte_util_db::def_table;

def_table! {
    /// Own current consensus version
    ///
    /// This is stored separately, in case `modules_configs` table ever needs to change,
    /// so that core-consensus-module can figure out easily own current version.
    self_version: () => ConsensusVersion
}

def_table! {
    /// Set of all voting consensus peers.
    ///
    /// Note: this is *the* current logical set of peers who can vote, from which
    /// `ConsensusParams` are derived. Notably the lower level `consensus`
    /// applies changes consensus membership changes with a delay, to accommodate
    /// finalization delay.
    peers: PeerPubkey => ()
}

def_table! {
    /// Tracks which new peers existing peers would like to add to the consensus voting.
    add_peer_votes: PeerPubkey /* voter */ => PeerPubkey /* voted to be added */
}

def_table! {
    /// Tracks which peers existing peers would like to remove from the consensus voting.
    remove_peer_votes: PeerPubkey /* voter */ => PeerPubkey /* voted to be removed */
}

def_table! {
    /// Our own pending vote to add new peer which we want to propose
    ///
    /// Once it is processed as a consensus item, it will update `add_peers_votes` table.
    pending_add_peer_vote: () => PeerPubkey
}

def_table! {
    /// Our own pending vote to remove a peer which we want to propose
    ///
    /// Once it is processed as a consensus item, it will update `remove_peers_votes` table.
    pending_remove_peer_vote: () => PeerPubkey
}

def_table! {
    /// Current list of all initialized modules, along with their configuration
    modules_configs: ModuleId => ModuleConfig
}

def_table! {
    modules_versions_votes: (PeerPubkey, ModuleId) => ConsensusVersion
}

def_table! {
    pending_modules_versions_votes: (ModuleId) => ConsensusVersionMinor
}

def_table! {
    /// Tracks which new modules existing peers would like to add
    add_module_votes: PeerPubkey /* voter */ => (ModuleKind, ConsensusVersion) /* voted to be added */
}

def_table! {
    /// Our own pending vote to add new module which we want to propose
    ///
    /// Once it is processed as a consensus item, it will update `add_module_votes` table.
    pending_add_module_vote: () => (ModuleKind, ConsensusVersion)
}
