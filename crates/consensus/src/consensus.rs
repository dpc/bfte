mod ctx;
mod finish_round;
mod getters;
mod handle_finality_vote;
mod handle_notarized_block;
mod handle_vote;
mod init;
mod version;

use std::sync::Arc;

use bfte_consensus_core::block::BlockRound;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_db::Database;
use tokio::sync::watch;

pub use self::init::*;

const LOG_TARGET: &str = "bfte::consensus";

pub struct Consensus {
    /// Database the consensus stores its state
    db: Arc<Database>,

    /// Own [`PeerPubkey`], `None` if the peer does not and can not participate
    /// in the consensu and is just a non-voting replica.
    our_peer_pubkey: Option<PeerPubkey>,
    current_round_with_timeout_tx: watch::Sender<(BlockRound, bool)>,
    current_round_with_timeout_rx: watch::Receiver<(BlockRound, bool)>,

    /// The consensus on the finality height
    ///
    /// Notably: does not mean that current peer actually has all the blocks up
    /// this point yet.
    finality_consensus_tx: watch::Sender<BlockRound>,
    finality_consensus_rx: watch::Receiver<BlockRound>,

    /// Block round past last notarized block
    finality_self_vote_tx: watch::Sender<BlockRound>,
    finality_self_vote_rx: watch::Receiver<BlockRound>,

    /// Notifications every new vote
    new_votes_tx: watch::Sender<()>,
    new_votes_rx: watch::Receiver<()>,

    /// Notifications every new proposal
    new_proposal_tx: watch::Sender<()>,
    new_proposal_rx: watch::Receiver<()>,
}
