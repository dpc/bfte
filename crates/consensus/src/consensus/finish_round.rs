use bfte_consensus_core::block::BlockRound;
use bfte_db::ctx::WriteTransactionCtx;
use bfte_db::error::{DbTxResult, TxSnafu};
use snafu::{ResultExt as _, Snafu};
use tracing::{debug, info, warn};

use super::Consensus;
use super::ctx::ConsensusReadDbOps as _;
use crate::consensus::LOG_TARGET;
use crate::consensus::ctx::ConsensusWriteDbOps as _;

#[derive(Snafu, Debug)]
pub enum RoundInvariantError {
    PinnedMismatch,
}

impl Consensus {
    pub(crate) fn check_round_end(
        &self,
        ctx: &WriteTransactionCtx,
        mut cur_round: BlockRound,
    ) -> DbTxResult<(), RoundInvariantError> {
        let cur_round_start = cur_round;
        let mut new_last_notarized_round = None;

        let highest_notarized_block = ctx.get_prev_notarized_block(BlockRound::MAX)?;

        let cur_round_needs_a_timeout = loop {
            if cur_round_start != cur_round {
                // Every time we advance the round, we check for existing block pins
                let prev_round = cur_round.prev().expect("Can't underflow");
                if let Some(pinned_hash) = ctx.get_pinned_block(prev_round)? {
                    if Some(pinned_hash) != ctx.get_notarized_block(prev_round)?.map(|b| b.hash()) {
                        return PinnedMismatchSnafu.fail().context(TxSnafu);
                    }
                }
            }
            // Notarized blocks have highest priority, so if we have
            // one for this round or higher, we automatically advance
            if highest_notarized_block.is_some_and(|highest| cur_round <= highest.round) {
                debug!(
                    target: LOG_TARGET,
                    round = %cur_round,
                    "Already have notarized block for the current or later round"
                );
                new_last_notarized_round = Some(cur_round);
                cur_round = cur_round.next_expect();
                continue;
            }

            let mut needs_a_timeout = false;
            let consensus_param = ctx.get_consensus_params(cur_round)?;
            let threshold = consensus_param.num_peers().threshold();

            // If we're here, we have no notarized vote higher or equal
            // so we check if the current proposal reached the treashold

            if let Some(proposal) = ctx.get_proposal(cur_round)? {
                // If we have a block we always want to have a timeout on
                needs_a_timeout |= true;
                let num_votes_proposal = ctx.get_num_votes_proposal(cur_round)?;
                if threshold <= num_votes_proposal {
                    info!(
                        target: LOG_TARGET,
                        round = %cur_round,
                        %num_votes_proposal,
                        "Round produced a block"
                    );
                    new_last_notarized_round = Some(cur_round);
                    ctx.insert_notarized_block(cur_round, proposal, None)?;
                    cur_round = cur_round.next_expect();
                    continue;
                }
            }

            // We don't have any notarized blocks, the proposal
            // didn't reach a `threshold` so maybe dummy votes did.

            let num_votes_dummy = ctx.get_num_votes_dummy(cur_round)?;
            if threshold <= num_votes_dummy {
                warn!(
                    target: LOG_TARGET,
                    round = %cur_round,
                    %num_votes_dummy,
                    "Round produced a dummy"
                );
                cur_round = cur_round.next_expect();
                continue;
            }

            if consensus_param.num_peers().max_faulty() < num_votes_dummy {
                // Seems that enough peers wants the round to go on
                // so we should have a timeout on too
                needs_a_timeout |= true;
            }

            if ctx
                .get_vote_dummy(cur_round, consensus_param.leader_idx(cur_round))?
                .is_some()
            {
                // If we have a timeout from the leader, we need a timeout
                needs_a_timeout |= true;
            }
            break needs_a_timeout;
        };

        // Advancing is relatively uncommon, so skip an update if nothing changed
        if cur_round_start != cur_round {
            debug!(
                target: LOG_TARGET,
                prev_round = %cur_round_start,
                round = %cur_round,
                "Round advanced"
            );

            ctx.set_current_round(cur_round)?;
        }

        if let Some(new_last_notarized_round) = new_last_notarized_round {
            let finality_self_vote = new_last_notarized_round.next_expect();
            let finality_self_vote_tx = self.finality_self_vote_tx.clone();
            ctx.on_commit(move || {
                finality_self_vote_tx.send_replace(finality_self_vote);
            });

            if let Some(our_peer_pubkey) = self.our_peer_pubkey {
                // Since we don't query ourselves for own finality vote, we
                // update it here.
                self.update_peer_finality_vote_round(
                    ctx,
                    cur_round,
                    our_peer_pubkey,
                    finality_self_vote,
                )?;
            }
        }

        // Similarly, we only need to update notification if the round
        // was updated, or the timeout was set to `true`
        if cur_round != cur_round_start || cur_round_needs_a_timeout {
            let round_timeout_tx = self.current_round_with_timeout_tx.clone();
            ctx.on_commit(move || {
                round_timeout_tx.send_if_modified(|value| {
                    let prev = *value;
                    if value.0 == cur_round && value.1 {
                        // If we already had a timeout set, we should never revert
                        debug_assert!(cur_round_needs_a_timeout);
                        return false;
                    }
                    *value = (
                        cur_round,
                        // TODO: make the timeout duration exponential based on how many
                        // unfinalized rounds we already have
                        cur_round_needs_a_timeout,
                    );
                    prev != *value
                });
            });
        }

        Ok(())
    }
}
