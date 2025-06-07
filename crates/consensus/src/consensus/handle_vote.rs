use bfte_consensus_core::Signature;
use bfte_consensus_core::block::{
    BlockHeader, BlockPayloadRaw, BlockRound, VerifyWithContentError,
};
use bfte_consensus_core::msg::WaitVoteResponse;
use bfte_consensus_core::peer::PeerIdx;
use bfte_db::ctx::WriteTransactionCtx;
use bfte_db::error::{DbTxError, TxSnafu};
use snafu::{OptionExt as _, ResultExt as _, Snafu};
use tracing::{Level, debug, instrument};

use super::Consensus;
use super::finish_round::RoundInvariantError;
use crate::consensus::{
    ConsensusReadDbOps as _, ConsensusWriteDbOps as _, InsertOutcome, LOG_TARGET,
};

#[derive(Debug, Snafu)]
pub enum ProcessVoteError {
    #[snafu(display("Invalid Round - expected: {expected}, received: {received}"))]
    InvalidRound {
        expected: BlockRound,
        received: BlockRound,
    },
    InvalidSignature,
    ForkedProposal {
        peer_idx: PeerIdx,
        existing: BlockHeader,
    },
    ForkedSignature {
        peer_idx: PeerIdx,
        existing: Signature,
    },
    VoteForADifferentProposal,
    NotALeader,
    #[snafu(display("Proposed a dummy block"))]
    Dummy,
    InvalidContent {
        source: VerifyWithContentError,
    },
    #[snafu(transparent)]
    RoundInvariant {
        source: RoundInvariantError,
    },
    DoesNotExtend {
        prev: Option<BlockHeader>,
    },
}

type ProcessVoteResult<T> = Result<T, ProcessVoteError>;

impl Consensus {
    pub async fn process_vote_response(
        &self,
        peer_idx: PeerIdx,
        resp: WaitVoteResponse,
    ) -> ProcessVoteResult<()> {
        self.db
            .write_with_expect_falliable(|ctx| self.process_vote_response_tx(ctx, peer_idx, resp))
            .await?;
        Ok(())
    }

    #[instrument(skip_all,
        fields(
            peer_idx = %peer_idx,
            block_round = %resp.block().round,
            is_dummy = %resp.block().is_dummy(),
            is_proposal = %resp.is_proposal(),
        ),
        ret(level = Level::DEBUG))]
    fn process_vote_response_tx(
        &self,
        ctx: &WriteTransactionCtx,
        peer_idx: PeerIdx,
        resp: WaitVoteResponse,
    ) -> Result<(), DbTxError<ProcessVoteError>> {
        let cur_round = ctx.get_current_round()?;

        debug_assert!(
            ctx.get_notarized_block(cur_round)?.is_none(),
            "We should not have notarized blocks for the current round"
        );

        // Expecting only votes for the current round
        if resp.block().round != cur_round {
            debug!(target: LOG_TARGET, %cur_round, "Invalid round");
            return Err(ProcessVoteError::InvalidRound {
                expected: cur_round,
                received: resp.block().round,
            })
            .context(TxSnafu)?;
        }

        // Given the round, look up peers for it
        let consensus_params = ctx.get_consensus_params(cur_round)?;
        let consensus_params_hash = consensus_params.hash();
        let consensus_params_len = consensus_params.len();

        let vote = match resp {
            WaitVoteResponse::Vote { block } => block,
            WaitVoteResponse::Proposal { block, payload } => {
                // Only leader can propose a block
                if consensus_params.leader_idx(cur_round) != peer_idx {
                    return Err(ProcessVoteError::NotALeader).context(TxSnafu)?;
                }

                // Illegal to propose a dummy (just vote dummy instead)
                if block.is_dummy() {
                    return Err(ProcessVoteError::Dummy).context(TxSnafu)?;
                }

                block
                    .verify_with_content(consensus_params_hash, consensus_params_len, &payload)
                    .context(InvalidContentSnafu)
                    .context(TxSnafu)?;

                // Proposal must extend last (non-dummy) notarized block
                let prev_block = ctx.get_prev_notarized_block(cur_round)?;
                if !block.inner.does_directly_extend(prev_block) {
                    return Err(ProcessVoteError::DoesNotExtend { prev: prev_block })
                        .context(TxSnafu)?;
                };

                match ctx.insert_block_proposal(cur_round, block.inner, &payload)? {
                    InsertOutcome::AlreadyPresent(existing) => {
                        if existing != block.inner {
                            return Err(ProcessVoteError::ForkedProposal { existing, peer_idx })
                                .context(TxSnafu)?;
                        } else {
                            return Ok(());
                        }
                    }
                    InsertOutcome::Inserted => {
                        // proposal inserted, now count it as a first
                        // vote too
                        ctx.on_commit({
                            let new_proposal_tx = self.new_proposal_tx.clone();
                            move || {
                                new_proposal_tx.send_replace(());
                            }
                        });
                    }
                }
                block
            }
        };

        // Handle both proposal and a vote as a vote

        // Check signature (this will revert inserting a proposal, so OK to do
        // afterwards)
        vote.verify_sig_peer_idx(peer_idx, &consensus_params.peers)
            .ok()
            .context(InvalidSignatureSnafu)
            .context(TxSnafu)?;

        let vote_insert_outcome = if vote.inner.is_dummy() {
            vote.inner
                .verify_with_content(
                    consensus_params_hash,
                    consensus_params_len,
                    &BlockPayloadRaw::empty(),
                )
                .context(InvalidContentSnafu)
                .context(TxSnafu)?;
            debug_assert_eq!(
                vote.inner,
                BlockHeader::new_dummy(vote.inner.round, &consensus_params)
            );
            ctx.insert_dummy_vote(cur_round, peer_idx, vote.sig)?
        } else {
            if let Some(proposal) = ctx.get_proposal(cur_round)? {
                if proposal != vote.inner {
                    VoteForADifferentProposalSnafu.fail().context(TxSnafu)?;
                }
            }
            ctx.insert_block_vote(cur_round, peer_idx, vote)?
                .map(|x| x.sig)
        };

        self.notify_new_votes(ctx);

        match vote_insert_outcome {
            InsertOutcome::Inserted => {}
            InsertOutcome::AlreadyPresent(existing) => {
                if existing != vote.sig {
                    return Err(ProcessVoteError::ForkedSignature { peer_idx, existing })
                        .context(TxSnafu)?;
                } else {
                    return Ok(());
                }
            }
        }

        self.check_round_end(ctx, cur_round)
            .map_err(DbTxError::tx_into)?;
        Ok(())
    }

    pub(crate) fn notify_new_votes(&self, ctx: &WriteTransactionCtx) {
        ctx.on_commit({
            let new_votes_tx = self.new_votes_tx.clone();

            move || {
                new_votes_tx.send_replace(());
            }
        });
    }
}
