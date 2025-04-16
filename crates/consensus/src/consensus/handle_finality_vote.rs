use bfte_consensus_core::block::{BlockHeader, BlockRound};
use bfte_consensus_core::msg::WaitFinalityVoteResponse;
use bfte_consensus_core::num_peers::ToNumPeers as _;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_consensus_core::signed::InvalidSignatureError;
use bfte_db::ctx::WriteTransactionCtx;
use bfte_db::error::{DbResult, DbTxError, TxSnafu};
use bfte_util_fmt_opt::AsFmtOption as _;
use snafu::{ResultExt as _, Snafu};
use tracing::{Level, debug, instrument, warn};

use super::Consensus;
use crate::consensus::{ConsensusReadDbOps as _, ConsensusWriteDbOps as _, LOG_TARGET};

#[derive(Debug, Snafu)]
pub enum ProcessFirstUnnotarizedUpdateError {
    InvalidSignatures { source: InvalidSignatureError },
}
type ProcessFirstUnnotarizedUpdateResult<T> = Result<T, ProcessFirstUnnotarizedUpdateError>;

impl Consensus {
    pub async fn process_finality_vote_update_response(
        &self,
        peer_pubkey: PeerPubkey,
        resp: WaitFinalityVoteResponse,
    ) -> ProcessFirstUnnotarizedUpdateResult<()> {
        self.db
            .write_with_expect_falliable(|ctx| {
                self.process_finality_vote_update_response_tx(ctx, peer_pubkey, resp)
            })
            .await?;
        Ok(())
    }

    #[instrument(skip_all,
        fields(%peer_pubkey, %round = update.inner.0),
        ret(level = Level::DEBUG))]
    fn process_finality_vote_update_response_tx(
        &self,
        ctx: &WriteTransactionCtx,
        peer_pubkey: PeerPubkey,
        WaitFinalityVoteResponse { update }: WaitFinalityVoteResponse,
    ) -> Result<(), DbTxError<ProcessFirstUnnotarizedUpdateError>> {
        let cur_round = ctx.get_current_round()?;

        update
            .verify_sig_peer_pubkey(peer_pubkey)
            .context(InvalidSignaturesSnafu)
            .context(TxSnafu)?;
        self.update_peer_finality_vote_round(ctx, cur_round, peer_pubkey, update.inner.0)?;
        Ok(())
    }

    pub(crate) fn update_peer_last_notarized_block(
        &self,
        ctx: &WriteTransactionCtx,
        cur_round: BlockRound,
        peer_pubkey: PeerPubkey,
        block: &BlockHeader,
    ) -> DbResult<()> {
        // First unnotarized round is the next round after last notarized block
        let unnotarized_round = block.round.next().expect("Can't overflow");

        self.update_peer_finality_vote_round(ctx, cur_round, peer_pubkey, unnotarized_round)
    }
    /// Track finalization updates as peers confirm their notarizations
    pub(crate) fn update_peer_finality_vote_round(
        &self,
        ctx: &WriteTransactionCtx,
        cur_round: BlockRound,
        peer_pubkey: PeerPubkey,
        finality_vote_round: BlockRound,
    ) -> DbResult<()> {
        let prev_finality_vote_round =
            ctx.update_finality_vote(peer_pubkey, finality_vote_round)?;

        if prev_finality_vote_round.is_some_and(|prev| finality_vote_round < prev) {
            warn!(
                target: LOG_TARGET,
                %peer_pubkey,
                prev = %prev_finality_vote_round.fmt_option(),
                curr = %finality_vote_round,
                "Peer's finality vote went backwards"
            );
            return Ok(());
        }

        if prev_finality_vote_round.is_some_and(|prev| finality_vote_round == prev) {
            // Nothing changed, no need to do anything
            return Ok(());
        }

        let cur_round_consensus_params = ctx.get_consensus_params(cur_round)?;

        let mut votes = vec![];

        for peer_pubkey in &cur_round_consensus_params.peers {
            votes.push(ctx.get_finality_vote(*peer_pubkey)?.unwrap_or_default());
        }

        votes.sort();

        let num_peers = cur_round_consensus_params.peers.to_num_peers();
        let finality_cons = votes[num_peers.max_faulty()];

        let prev_finality_cons = ctx
            .update_finality_consensus(finality_cons)?
            .unwrap_or_default();

        if finality_cons < prev_finality_cons {
            panic!("Finalization went backwards: {finality_cons} < {prev_finality_cons}");
        }

        let tx = self.finality_cons_tx.clone();

        if prev_finality_cons != finality_cons {
            ctx.on_commit(move || {
                debug!(target: LOG_TARGET, round = %finality_cons, "New finality consensus");
                tx.send_replace(finality_cons);
            });
        }

        Ok(())
    }
}
