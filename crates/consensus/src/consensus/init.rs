use std::sync::Arc;

use bfte_consensus_core::block::{BlockHash, BlockRound};
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_db::Database;
use bfte_db::ctx::WriteTransactionCtx;
use bfte_db::error::{DbError, DbResult, DbTxError, TxSnafu};
use snafu::{ResultExt as _, Snafu};

use super::{Consensus, ConsensusReadDbOps as _, ConsensusWriteDbOps as _, InsertOutcome};
use crate::tables::{
    cons_blocks_notarized, cons_blocks_pinned, cons_blocks_proposals, cons_finality_consensus,
    cons_finality_votes, cons_votes_block, cons_votes_dummy,
};

#[derive(Debug, Snafu)]
pub enum InitError {
    AlreadyInitialized,
}

type InitResult<T> = Result<T, InitError>;

#[derive(Debug, Snafu)]
pub enum OpenError {
    NotInitialized,
}
type OpenResult<T> = Result<T, OpenError>;

impl Consensus {
    /// Init database by creating or joining a new federation of a given
    /// starting [`ConsensuParams`]
    pub async fn init(
        params: &ConsensusParams,
        db: Arc<Database>,
        our_peer_pubkey: Option<PeerPubkey>,
        pinned: Option<(BlockRound, BlockHash)>,
    ) -> InitResult<Self> {
        let current_round = {
            db.write_with_expect_falliable(|ctx| Self::init_tx(ctx, params, pinned))
                .await?
        };

        Ok(Self::open_internal(current_round, db, our_peer_pubkey).await)
    }

    pub async fn open(db: Arc<Database>, our_peer_pubkey: Option<PeerPubkey>) -> OpenResult<Self> {
        let cur_round = db.write_with_expect_falliable(Self::open_tx).await?;

        Ok(Self::open_internal(cur_round, db, our_peer_pubkey).await)
    }

    async fn open_internal(
        cur_round: BlockRound,
        db: Arc<Database>,
        our_peer_pubkey: Option<PeerPubkey>,
    ) -> Self {
        let (round_timeout_tx, round_timeout_rx) = tokio::sync::watch::channel((cur_round, None));
        let (new_votes_tx, new_votes_rx) = tokio::sync::watch::channel(());
        let (new_proposal_tx, new_proposal_rx) = tokio::sync::watch::channel(());
        let first_unfinalized_round = db
            .read_with_expect(|ctx| ctx.get_finality_consensus())
            .await
            .unwrap_or_default();
        let (finality_cons_tx, finality_cons_rx) =
            tokio::sync::watch::channel(first_unfinalized_round);

        let s = Self {
            db,
            current_round_with_timeout_start_rx: round_timeout_rx,
            current_round_with_timeout_start_tx: round_timeout_tx,
            finality_cons_tx,
            finality_cons_rx,
            our_peer_pubkey,
            new_votes_tx,
            new_votes_rx,
            new_proposal_tx,
            new_proposal_rx,
        };

        // This will mostly calculate a correct timeout again, based on the state
        // of the database.
        s.db.write_with_expect_falliable(|ctx| s.check_round_end(ctx, cur_round))
            .await
            .expect("Database should be in consistent state when opening");

        s
    }

    fn init_tx(
        ctx: &WriteTransactionCtx,
        params: &ConsensusParams,
        pinned: Option<(BlockRound, BlockHash)>,
    ) -> Result<BlockRound, DbTxError<InitError>> {
        if let InsertOutcome::AlreadyPresent(existing) =
            ctx.insert_consensus_params(BlockRound::ZERO, params)?
        {
            if existing != params.hash() {
                return AlreadyInitializedSnafu.fail().context(TxSnafu);
            }
        }
        if let Some((round, hash)) = pinned {
            let mut tbl = ctx
                .open_table(&cons_blocks_pinned::TABLE)
                .map_err(DbError::from)?;
            tbl.insert(&round, &hash).map_err(DbError::from)?;
        }

        Self::init_tables_tx(ctx)?;

        Ok(ctx.get_current_round()?)
    }

    fn open_tx(ctx: &WriteTransactionCtx) -> Result<BlockRound, DbTxError<OpenError>> {
        let Some(_) = ctx.get_consensus_params_opt(BlockRound::ZERO)? else {
            return NotInitializedSnafu.fail().context(TxSnafu);
        };

        Self::init_tables_tx(ctx)?;

        Ok(ctx.get_current_round()?)
    }

    fn init_tables_tx(tx: &WriteTransactionCtx) -> DbResult<()> {
        tx.open_table(&cons_blocks_proposals::TABLE)?;
        tx.open_table(&cons_blocks_notarized::TABLE)?;
        tx.open_table(&cons_votes_dummy::TABLE)?;
        tx.open_table(&cons_votes_block::TABLE)?;
        tx.open_table(&cons_finality_consensus::TABLE)?;
        tx.open_table(&cons_finality_votes::TABLE)?;
        Ok(())
    }
}
