use bfte_consensus_core::block::{
    BlockHeader, BlockPayloadRaw, BlockRound, VerifyWithContentError,
};
use bfte_consensus_core::msg::WaitNotarizedBlockResponse;
use bfte_consensus_core::peer::PeerIdx;
use bfte_consensus_core::signed::{InvalidNotarizationError, Notarized, Signed};
use bfte_db::ctx::WriteTransactionCtx;
use bfte_db::error::{DbTxError, TxSnafu};
use bfte_util_fmt_opt::AsFmtOption as _;
use snafu::{ResultExt as _, Snafu};
use tracing::{Level, debug, instrument};

use super::Consensus;
use super::finish_round::RoundInvariantError;
use crate::consensus::{
    ConsensusReadDbOps as _, ConsensusWriteDbOps as _, InsertOutcome, LOG_TARGET,
};

#[derive(Debug, Snafu)]
pub enum ProcessNotarizedBlockError {
    InvalidSignatures {
        source: InvalidNotarizationError,
    },
    WrongRoundDummy,
    WrongRoundBlock {
        required: BlockRound,
        received: BlockRound,
    },
    WrongParent,
    #[snafu(transparent)]
    RoundInvariant {
        source: RoundInvariantError,
    },
    InvalidNotarizedContent {
        source: VerifyWithContentError,
    },
}

impl ProcessNotarizedBlockError {
    pub fn is_fatal(&self) -> bool {
        match self {
            // Invalid notarized content means the whole
            // federation messed up (or we did), very badly.
            ProcessNotarizedBlockError::InvalidNotarizedContent { source: _ } => true,
            _ => false,
        }
    }
}
type ProcessNotarizedBlockResult<T> = Result<T, ProcessNotarizedBlockError>;

impl Consensus {
    pub async fn process_notarized_block_response(
        &self,
        peer_idx: PeerIdx,
        resp: WaitNotarizedBlockResponse,
    ) -> ProcessNotarizedBlockResult<()> {
        self.db
            .write_with_expect_falliable(|ctx| {
                self.process_notarized_block_response_tx(ctx, peer_idx, resp)
            })
            .await?;
        Ok(())
    }

    #[instrument(skip_all,
        fields(peer_idx = %recv_peer_idx, block_round = %block.round, is_dummy = %block.is_dummy()),
        ret(level = Level::DEBUG))]
    fn process_notarized_block_response_tx(
        &self,
        ctx: &WriteTransactionCtx,
        recv_peer_idx: PeerIdx,
        WaitNotarizedBlockResponse { block, payload }: WaitNotarizedBlockResponse,
    ) -> Result<(), DbTxError<ProcessNotarizedBlockError>> {
        let cur_round = ctx.get_current_round()?;

        let block_round_consensus_params = ctx.get_consensus_params(block.round)?;
        let block_round_consensus_params_hash = block_round_consensus_params.hash();
        let block_round_consensus_params_len = block_round_consensus_params.len();

        block
            .verify_sigs(&block_round_consensus_params)
            .context(InvalidSignaturesSnafu)
            .context(TxSnafu)?;

        block
            .verify_with_content(
                block_round_consensus_params_hash,
                block_round_consensus_params_len,
                &payload,
            )
            .context(InvalidNotarizedContentSnafu)
            .context(TxSnafu)?;

        if block.is_dummy() {
            // We only accept notarized dummy blocks for the current round
            if block.round != cur_round {
                WrongRoundDummySnafu.fail().context(TxSnafu)?;
            }
            debug!(
                target: LOG_TARGET,
                "New notarized dummy block our previous notarized block"
            );
            for (peer_idx, sig) in block.sigs {
                ctx.insert_dummy_vote(cur_round, peer_idx, sig)?;
            }
            self.notify_new_votes(ctx);
            self.check_round_end(ctx, cur_round)
                .map_err(DbTxError::tx_into)?;
            debug_assert!(
                cur_round.next().expect("Can't run out") <= ctx.get_current_round()?,
                "Must advance the round after notarized dummy vote received"
            );
            return Ok(());
        }

        let our_latest_notarized_block = ctx.get_prev_notarized_block(cur_round)?;
        debug_assert!(our_latest_notarized_block.is_none_or(|b| b.round < cur_round));
        let block_round = block.round;

        if let Some(our_latest) = our_latest_notarized_block {
            // The block needs to be higher than our current highest notarized block
            // for us to consider it.
            if block_round == our_latest.round {
                // There's no point in sending error
                if our_latest != block.inner {
                    panic!("Consensus failure: Two different notarized blocks at the same height");
                }
                return Ok(());
            }
            if block_round <= our_latest.round {
                WrongRoundBlockSnafu {
                    required: our_latest.round,
                    received: block_round,
                }
                .fail()
                .context(TxSnafu)?;
            }
        }

        if block.inner.does_directly_extend(our_latest_notarized_block) {
            debug!(
                target: LOG_TARGET,
                prev_notarized_round = %our_latest_notarized_block.map(|b| b.round).fmt_option(),
                "New notarized block extending our previous notarized block"
            );

            self.insert_notarized_block(ctx, cur_round, block, payload)?;
        } else {
            debug!(
                target: LOG_TARGET,
                prev_notarized_round = %our_latest_notarized_block.map(|b| b.round).fmt_option(),
                "New notarized block NOT extending our previous notarized block."
            );
            // Delete previous notarized block as it seems divergent, and rewind the history
            // one block.
            // This is somewhat inneficient, but should not be happening too often.
            // And is conceptually simple.
            //
            // If we received a notarized block and it does not extend our latest notarized
            // block, then we *MUST* have previous block, or something is very wrong.
            let our_latest_notarized_block_round = our_latest_notarized_block
                .map(|b| b.round)
                .expect("Must have latest notarized");

            // Deleting one notarized block should always be enough
            // because we can't have multiple notarized block without
            // `threshold` of peers voting for the sebusequent ones
            // (thus being aware of their parents).
            //
            // But the peer could have lied, and send us not the first, but further
            // notarized block, so we need to check, before we act
            let our_second_latest_notarized =
                ctx.get_prev_notarized_block(our_latest_notarized_block_round)?;

            debug_assert!(
                our_second_latest_notarized
                    .is_none_or(|b| b.round < our_latest_notarized_block_round)
            );

            if !block
                .inner
                .does_directly_extend(our_second_latest_notarized)
            {
                WrongParentSnafu.fail().context(TxSnafu)?;
            }

            ctx.delete_notarized_block(our_latest_notarized_block_round)?;

            self.insert_notarized_block(ctx, cur_round, block, payload)?;
        }

        self.check_round_end(ctx, cur_round)
            .map_err(DbTxError::tx_into)?;

        Ok(())
    }

    fn insert_notarized_block(
        &self,
        ctx: &WriteTransactionCtx,
        cur_round: BlockRound,
        block: Notarized<BlockHeader>,
        payload: BlockPayloadRaw,
    ) -> Result<(), DbTxError<ProcessNotarizedBlockError>> {
        let outcome = ctx.insert_notarized_block(block.round, block.inner, Some(&payload))?;
        debug_assert_eq!(outcome, InsertOutcome::Inserted);
        let block_round = block.round;
        for (peer_idx, sig) in block.sigs {
            ctx.insert_block_vote(
                block_round,
                peer_idx,
                Signed {
                    inner: block.inner,
                    sig,
                },
            )?;
        }
        self.notify_new_votes(ctx);
        if let Some(our_peer_pubkey) = self.our_peer_pubkey {
            self.update_peer_last_notarized_block(ctx, cur_round, our_peer_pubkey, &block.inner)?;
        }
        Ok(())
    }
}
