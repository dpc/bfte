use std::time::Duration;

use bfte_consensus_core::block::{BlockHeader, BlockPayloadHash, BlockPayloadRaw, BlockRound};
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::msg::{
    WaitNotarizedBlockRequest, WaitNotarizedBlockResponse, WaitVoteResponse,
};
use bfte_consensus_core::peer::{PeerIdx, PeerPubkey};
use bfte_consensus_core::peer_set::PeerSet;
use bfte_consensus_core::signed::{Notarized, Signed};
use bfte_consensus_core::timestamp::Timestamp;
use bfte_db::ctx::WriteTransactionCtx;
use bfte_db::error::DbResult;
use tokio::sync::watch;
use tracing::{info, warn};

use super::{Consensus, ConsensusReadDbOps as _};
use crate::consensus::{ConsensusWriteDbOps as _, LOG_TARGET};
use crate::tables::{cons_blocks_notarized, cons_blocks_payloads};
use crate::vote_set::VoteSet;

impl Consensus {
    pub fn current_round_with_timeout_start_rx(
        &self,
    ) -> watch::Receiver<(BlockRound, Option<Duration>)> {
        self.current_round_with_timeout_start_rx.clone()
    }

    pub fn finality_consensus_rx(&self) -> watch::Receiver<BlockRound> {
        self.finality_cons_rx.clone()
    }

    pub fn new_votes_rx(&self) -> watch::Receiver<()> {
        self.new_votes_rx.clone()
    }

    pub fn new_proposal_rx(&self) -> watch::Receiver<()> {
        self.new_proposal_rx.clone()
    }

    pub async fn get_current_round(&self) -> BlockRound {
        self.db
            .read_with_expect(|ctx| ctx.get_current_round())
            .await
    }

    pub async fn get_prev_notarized_block(&self, round: BlockRound) -> Option<BlockHeader> {
        self.db
            .read_with_expect(|ctx| ctx.get_prev_notarized_block(round))
            .await
    }

    pub async fn get_next_notarized_block(&self, round: BlockRound) -> Option<BlockHeader> {
        self.db
            .read_with_expect(|ctx| ctx.get_next_notarized_block(round))
            .await
    }
    pub async fn get_block_payload(
        &self,
        block_payload_hash: BlockPayloadHash,
    ) -> Option<BlockPayloadRaw> {
        self.db
            .read_with_expect(|ctx| ctx.get_block_payload(block_payload_hash))
            .await
    }

    pub async fn get_proposal(&self, round: BlockRound) -> Option<BlockHeader> {
        self.db
            .read_with_expect(|ctx| ctx.get_proposal(round))
            .await
    }

    pub async fn get_finality_consensus(&self) -> Option<BlockRound> {
        self.db
            .read_with_expect(|ctx| ctx.get_finality_consensus())
            .await
    }

    pub async fn get_finality_vote(&self, peer_pubkey: PeerPubkey) -> Option<BlockRound> {
        self.db
            .read_with_expect(|ctx| ctx.get_finality_vote(peer_pubkey))
            .await
    }

    /// This is very much tied to semantics of [`WaitNotarizedBlockRequest`]
    pub async fn get_notarized_block_resp(
        &self,
        req: WaitNotarizedBlockRequest,
    ) -> Option<WaitNotarizedBlockResponse> {
        self.db
            .read_with_expect(|ctx| {
                let tbl_notarized_blocks = ctx.open_table(&cons_blocks_notarized::TABLE)?;

                if let Some(block) = tbl_notarized_blocks
                    .range(req.min_notarized_round..)?
                    .next()
                    .transpose()?
                    .map(|(_k, v)| v.value())
                {
                    let tbl_payloads = ctx.open_table(&cons_blocks_payloads::TABLE)?;
                    let payload = tbl_payloads
                        .get(&block.payload_hash)?
                        .map(|g| g.value())
                        .expect("Must have payload for every notarized block");

                    let votes_block = ctx.get_votes_proposal(block.round)?;

                    let block = Notarized::new(block, votes_block);
                    debug_assert_eq!(
                        block.verify_sigs(&ctx.get_consensus_params(block.round)?),
                        Ok(())
                    );
                    return Ok(Some(WaitNotarizedBlockResponse { block, payload }));
                }

                let consensus_params = ctx.get_consensus_params(req.cur_round)?;
                let votes_dummy = ctx.get_votes_dummy(req.cur_round)?;

                if consensus_params.num_peers().threshold() <= votes_dummy.len() {
                    let block = BlockHeader::new_dummy(req.cur_round, &consensus_params);

                    let block = Notarized::new(block, votes_dummy);

                    debug_assert_eq!(block.verify_sigs(&consensus_params), Ok(()));

                    return Ok(Some(WaitNotarizedBlockResponse {
                        block,
                        payload: BlockPayloadRaw::empty(),
                    }));
                }

                Ok(None)
            })
            .await
    }

    pub async fn get_current_round_and_params(&self) -> (BlockRound, ConsensusParams) {
        self.db
            .read_with_expect(|ctx| {
                let round = ctx.get_current_round()?;
                let consensus_params = ctx.get_consensus_params(round)?;
                Ok((round, consensus_params))
            })
            .await
    }

    pub async fn get_consensus_params(&self, round: BlockRound) -> ConsensusParams {
        self.db
            .read_with_expect(|ctx| ctx.get_consensus_params(round))
            .await
    }

    pub async fn has_pending_consensus_params_change(&self, round: BlockRound) -> bool {
        self.db
            .read_with_expect(|ctx| ctx.has_pending_consensus_params_change(round))
            .await
    }

    /// Get [`ConsensusParams`] for the first round (`0`), which bootstrapped
    /// the consensus
    pub async fn get_init_params(&self) -> ConsensusParams {
        self.db
            .read_with_expect(|ctx| {
                let consensus_params = ctx.get_consensus_params(0.into())?;
                Ok(consensus_params)
            })
            .await
    }

    pub async fn get_peers_with_dummy_votes(&self, round: BlockRound) -> VoteSet {
        self.db
            .read_with_expect(|ctx| ctx.get_peers_with_dummy_votes(round))
            .await
    }

    pub async fn get_peers_with_proposal_votes(&self, round: BlockRound) -> VoteSet {
        self.db
            .read_with_expect(|ctx| ctx.get_peers_with_proposal_votes(round))
            .await
    }

    pub async fn get_vote(
        &self,
        round: BlockRound,
        round_consensus_params: &ConsensusParams,
        peer_idx: PeerIdx,
    ) -> Option<WaitVoteResponse> {
        self.db
            .read_with_expect(|ctx| {
                // Note: dummy votes are more important, as they are more "final"
                if let Some(sig) = ctx.get_vote_dummy(round, peer_idx)? {
                    return Ok(Some(WaitVoteResponse::Vote {
                        block: Signed::new(
                            BlockHeader::new_dummy(round, round_consensus_params),
                            sig,
                        ),
                    }));
                }
                if let Some(block) = ctx.get_vote_block(round, peer_idx)? {
                    if peer_idx == round.leader_idx(round_consensus_params.num_peers()) {
                        let Some(payload) = ctx.get_block_payload(block.inner.payload_hash)? else {
                            warn!(
                                target: LOG_TARGET,
                                %round,
                                payload_hash = %block.inner.payload_hash,
                                "Missing payload for proposal?!"
                            );
                            return Ok(None);
                        };
                        return Ok(Some(WaitVoteResponse::Proposal { block, payload }));
                    } else {
                        return Ok(Some(WaitVoteResponse::Vote { block }));
                    }
                }
                Ok(None)
            })
            .await
    }

    pub async fn get_round_params(&self, round: BlockRound) -> ConsensusParams {
        self.db
            .read_with_expect(|ctx| ctx.get_consensus_params(round))
            .await
    }
    pub async fn get_finalized_block(&self, round: BlockRound) -> Option<Notarized<BlockHeader>> {
        if *self.finality_cons_tx.borrow() <= round {
            return None;
        }
        let round_next = round.next()?;

        let block = self
            .db
            .read_with_expect(|ctx| {
                let Some(block) = ctx.get_prev_notarized_block(round_next)? else {
                    return Ok(None);
                };

                let votes_block = ctx.get_votes_proposal(block.round)?;
                let block = Notarized::new(block, votes_block);
                Ok(Some(block))
            })
            .await?;

        Some(block)
    }

    pub fn consensus_params_change_tx(
        &self,
        ctx: &WriteTransactionCtx,
        round: BlockRound,
        block_timestamp: Timestamp,
        new_peer_set: PeerSet,
    ) -> DbResult<()> {
        let current_params = ctx.get_consensus_params(round)?;

        let prev_mid_block = ctx.get_prev_notarized_block(round.half())?;

        let new_consensus_params = current_params.make_change(
            round,
            block_timestamp,
            new_peer_set,
            prev_mid_block.map(|b| (b.round, b.hash())),
        );

        info!(
            target: LOG_TARGET,
            schedule_round = %new_consensus_params.schedule_round,
            apply_round = %new_consensus_params.apply_round,
            peers_len = %new_consensus_params.peers.len(),
            "Scheduling consensus params change"
        );

        assert!(
            self.current_round_with_timeout_start_tx.borrow().0 < new_consensus_params.apply_round
        );

        ctx.insert_consensus_params(new_consensus_params.apply_round, &new_consensus_params)?;

        Ok(())
    }
}
