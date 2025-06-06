use bfte_consensus_core::module::ModuleId;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_module::module::config::ModuleConfig;
use bfte_util_db::def_table;

def_table! {
    modules_configs: ModuleId => ModuleConfig
}

def_table! {
    peers: PeerPubkey => ()
}

def_table! {
    add_peer_votes: PeerPubkey /* voter */ => PeerPubkey /* voted to be added */
}

def_table! {
    remove_peer_votes: PeerPubkey /* voter */ => PeerPubkey /* voted to be removed */
}

def_table! {
    pending_add_peer_vote: () => PeerPubkey
}
