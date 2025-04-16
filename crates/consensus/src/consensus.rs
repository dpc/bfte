mod finish_round;

mod generate_proposal;
mod getters;
mod handle_finality_vote;
mod handle_notarized_block;
mod handle_vote;
mod init;
mod version;

use std::sync::Arc;
use std::time::Duration;

use bfte_consensus_core::Signature;
use bfte_consensus_core::block::{
    BlockHash, BlockHeader, BlockPayloadHash, BlockPayloadRaw, BlockRound,
};
use bfte_consensus_core::consensus_params::{ConsensusParams, ConsensusParamsHash};
use bfte_consensus_core::peer::{PeerIdx, PeerPubkey};
use bfte_consensus_core::signed::Signed;
use bfte_consensus_core::vote::SignedVote;
use bfte_db::Database;
use bfte_db::ctx::WriteTransactionCtx;
use bfte_db::error::DbResult;
pub use init::{InitError, OpenError};
use redb_bincode::ReadTransaction;
use tokio::sync::watch;

use crate::tables::{
    cons_blocks_notarized, cons_blocks_payloads, cons_blocks_pinned, cons_blocks_proposals,
    cons_current_round, cons_finality_consensus, cons_finality_votes, cons_params,
    cons_params_schedule, cons_votes_block, cons_votes_dummy,
};
use crate::vote_set::VoteSet;

const LOG_TARGET: &str = "bfte::consensus";

pub struct Consensus {
    /// Database the consensus stores its state
    db: Arc<Database>,
    /// Own [`PeerPubkey`], `None` if the peer does not and can not participate
    /// in the consensu and is just a non-voting replica.
    our_peer_pubkey: Option<PeerPubkey>,
    current_round_with_timeout_start_tx: watch::Sender<(BlockRound, Option<Duration>)>,
    current_round_with_timeout_start_rx: watch::Receiver<(BlockRound, Option<Duration>)>,
    /// The consensus on the finality height
    ///
    /// Notably: does not mean that current peer actually has all the blocks up
    /// this point yet.
    finality_cons_tx: watch::Sender<BlockRound>,
    finality_cons_rx: watch::Receiver<BlockRound>,
    /// Notifications every new vote
    new_votes_tx: watch::Sender<()>,
    new_votes_rx: watch::Receiver<()>,
    /// Notifications every new proposal
    new_proposal_tx: watch::Sender<()>,
    new_proposal_rx: watch::Receiver<()>,
}

pub(crate) trait ConsensusReadDbOps {
    fn get_finalized_round(&self) -> DbResult<BlockRound>;
    fn get_current_round(&self) -> DbResult<BlockRound>;
    fn get_consensus_params(&self, round: BlockRound) -> DbResult<ConsensusParams> {
        Ok(self
            .get_consensus_params_opt(round)?
            .expect("Initialized consensus database must set consensus params for round 0"))
    }
    fn get_finality_consensus(&self) -> DbResult<Option<BlockRound>>;

    fn get_vote_dummy(&self, round: BlockRound, peer_idx: PeerIdx) -> DbResult<Option<Signature>>;
    fn get_vote_block(
        &self,
        round: BlockRound,
        peer_idx: PeerIdx,
    ) -> DbResult<Option<Signed<BlockHeader>>>;

    fn get_payload(&self, payload_hash: BlockPayloadHash) -> DbResult<Option<BlockPayloadRaw>>;
    fn get_finality_vote(&self, peer_pubkey: PeerPubkey) -> DbResult<Option<BlockRound>>;
    fn get_consensus_params_opt(&self, round: BlockRound) -> DbResult<Option<ConsensusParams>>;
    fn get_peers_with_proposal_votes(&self, round: BlockRound) -> DbResult<VoteSet>;
    fn get_peers_with_dummy_votes(&self, round: BlockRound) -> DbResult<VoteSet>;
    fn get_proposal(&self, round: BlockRound) -> DbResult<Option<BlockHeader>>;
    fn has_notarized_non_dummy_block(&self, round: BlockRound) -> DbResult<bool>;
    fn get_num_votes_dummy(&self, round: BlockRound) -> DbResult<usize>;
    fn get_num_votes_proposal(&self, round: BlockRound) -> DbResult<usize>;
    fn get_votes_dummy(&self, round: BlockRound) -> DbResult<Vec<(PeerIdx, Signature)>>;
    fn get_votes_proposal(&self, round: BlockRound) -> DbResult<Vec<(PeerIdx, Signature)>>;
    fn get_prev_notarized_block(&self, round: BlockRound) -> DbResult<Option<BlockHeader>>;
    fn get_notarized_block(&self, round: BlockRound) -> DbResult<Option<BlockHeader>>;
    fn get_pinned_block(&self, round: BlockRound) -> DbResult<Option<BlockHash>>;
}

pub(crate) trait ConsensusWriteDbOps {
    fn insert_consensus_params(
        &self,
        round: BlockRound,
        params: &ConsensusParams,
    ) -> DbResult<InsertOutcome<ConsensusParamsHash>>;
    /// Insert block and its payload if not already there
    ///
    /// Also scan all matching proposal votes and retain only ones that match
    /// the new block.
    fn insert_block_proposal(
        &self,
        round: BlockRound,
        block: BlockHeader,
        payload: &BlockPayloadRaw,
    ) -> DbResult<InsertOutcome<BlockHeader>>;

    fn insert_notarized_block(
        &self,
        round: BlockRound,
        block: BlockHeader,
        payload: Option<&BlockPayloadRaw>,
    ) -> DbResult<InsertOutcome<BlockHeader>>;

    fn insert_pinned_block(
        &self,
        round: BlockRound,
        hash: BlockHash,
    ) -> DbResult<InsertOutcome<BlockHash>>;

    fn update_finality_vote(
        &self,
        peer_pubkey: PeerPubkey,
        round: BlockRound,
    ) -> DbResult<Option<BlockRound>>;

    fn update_finality_consensus(&self, round: BlockRound) -> DbResult<Option<BlockRound>>;

    /// Delete a block at a given round
    fn delete_notarized_block(&self, round: BlockRound) -> DbResult<()>;

    fn insert_dummy_vote(
        &self,
        round: BlockRound,
        peerd_idx: PeerIdx,
        vote: Signature,
    ) -> DbResult<InsertOutcome<Signature>>;
    fn insert_block_vote(
        &self,
        round: BlockRound,
        peerd_idx: PeerIdx,
        vote: SignedVote,
    ) -> DbResult<InsertOutcome<SignedVote>>;
    fn set_current_round(&self, round: BlockRound) -> DbResult<()>;
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InsertOutcome<T> {
    Inserted,
    AlreadyPresent(T),
}

impl<T> InsertOutcome<T> {
    fn map<O>(self, f: impl FnOnce(T) -> O) -> InsertOutcome<O> {
        match self {
            InsertOutcome::Inserted => InsertOutcome::Inserted,
            InsertOutcome::AlreadyPresent(t) => InsertOutcome::AlreadyPresent((f)(t)),
        }
    }
}

macro_rules! impl_consensus_read_db_ops {
    ($t:ty) => {
        impl ConsensusReadDbOps for $t {
            fn get_finalized_round(&self) -> DbResult<BlockRound> {
                // let tbl = self.open_table(&cons_finalized_round_count::TABLE)?;

                // Ok(tbl.get(&())?.map(|g| g.value()).unwrap_or_default())
                todo!()
            }

            fn get_current_round(&self) -> DbResult<BlockRound> {
                let tbl = self.open_table(&cons_current_round::TABLE)?;

                Ok(tbl.get(&())?.map(|g| g.value()).unwrap_or_default())
            }

            fn get_vote_dummy(
                &self,
                round: BlockRound,
                peer_idx: PeerIdx,
            ) -> DbResult<Option<Signature>> {
                let tbl = self.open_table(&cons_votes_dummy::TABLE)?;

                Ok(tbl.get(&(round, peer_idx))?.map(|g| g.value()))
            }
            fn get_vote_block(
                &self,
                round: BlockRound,
                peer_idx: PeerIdx,
            ) -> DbResult<Option<Signed<BlockHeader>>> {
                let tbl = self.open_table(&cons_votes_block::TABLE)?;

                Ok(tbl.get(&(round, peer_idx))?.map(|g| g.value()))
            }

            fn get_peers_with_proposal_votes(&self, round: BlockRound) -> DbResult<VoteSet> {
                let mut vote_set = VoteSet::ZERO;

                let tbl_votes_block = self.open_table(&cons_votes_block::TABLE)?;

                for res in tbl_votes_block.range(&(round, PeerIdx::MIN)..=&(round, PeerIdx::MAX))? {
                    let (key, _) = res?;
                    vote_set.insert(key.value().1);
                }

                Ok(vote_set)
            }

            fn get_peers_with_dummy_votes(&self, round: BlockRound) -> DbResult<VoteSet> {
                let mut vote_set = VoteSet::ZERO;

                let tbl_votes_dummy = self.open_table(&cons_votes_dummy::TABLE)?;

                for res in tbl_votes_dummy.range(&(round, PeerIdx::MIN)..=&(round, PeerIdx::MAX))? {
                    let (key, _) = res?;
                    vote_set.insert(key.value().1);
                }
                Ok(vote_set)
            }

            fn get_consensus_params_opt(
                &self,
                round: BlockRound,
            ) -> DbResult<Option<ConsensusParams>> {
                let tbl = self.open_table(&cons_params_schedule::TABLE)?;

                let Some(hash) = tbl
                    .range(..=round)?
                    .next_back()
                    .transpose()?
                    .map(|g| g.1.value())
                else {
                    return Ok(None);
                };
                let tbl = self.open_table(&cons_params::TABLE)?;

                let params = tbl
                    .get(&hash)?
                    .map(|g| g.value())
                    .expect("Must always have params for a given hash");

                Ok(Some(params))
            }

            fn get_finality_consensus(&self) -> DbResult<Option<BlockRound>> {
                let tbl = self.open_table(&cons_finality_consensus::TABLE)?;

                Ok(tbl.get(&())?.map(|g| g.value()))
            }

            fn get_finality_vote(&self, peer_pubkey: PeerPubkey) -> DbResult<Option<BlockRound>> {
                let tbl = self.open_table(&cons_finality_votes::TABLE)?;

                Ok(tbl.get(&peer_pubkey)?.map(|g| g.value()))
            }

            fn get_payload(
                &self,
                payload_hash: BlockPayloadHash,
            ) -> DbResult<Option<BlockPayloadRaw>> {
                let tbl = self.open_table(&cons_blocks_payloads::TABLE)?;

                Ok(tbl.get(&payload_hash)?.map(|g| g.value()))
            }

            fn get_proposal(&self, round: BlockRound) -> DbResult<Option<BlockHeader>> {
                let tbl = self.open_table(&cons_blocks_proposals::TABLE)?;
                if let Some(v) = tbl.get(&round)? {
                    let v = v.value();
                    assert_eq!(v.round, round);

                    return Ok(Some(v));
                }

                Ok(None)
            }
            fn has_notarized_non_dummy_block(&self, round: BlockRound) -> DbResult<bool> {
                let tbl = self.open_table(&cons_blocks_notarized::TABLE)?;
                if tbl.get(&round)?.is_some() {
                    return Ok(true);
                }

                Ok(false)
            }

            fn get_notarized_block(&self, round: BlockRound) -> DbResult<Option<BlockHeader>> {
                let tbl_blocks = self.open_table(&cons_blocks_notarized::TABLE)?;

                Ok(tbl_blocks.get(&round)?.map(|v| v.value()))
            }

            fn get_pinned_block(&self, round: BlockRound) -> DbResult<Option<BlockHash>> {
                let tbl = self.open_table(&cons_blocks_pinned::TABLE)?;

                Ok(tbl.get(&round)?.map(|v| v.value()))
            }

            fn get_prev_notarized_block(&self, round: BlockRound) -> DbResult<Option<BlockHeader>> {
                let tbl_blocks = self.open_table(&cons_blocks_notarized::TABLE)?;

                Ok(tbl_blocks
                    .range(..round)?
                    .next_back()
                    .transpose()?
                    .map(|(_k, v)| v.value()))
            }

            fn get_num_votes_dummy(&self, round: BlockRound) -> DbResult<usize> {
                let tbl = self.open_table(&cons_votes_dummy::TABLE)?;

                Ok(tbl
                    .range(&(round, PeerIdx::MIN)..=&(round, PeerIdx::MAX))?
                    .count())
            }

            fn get_num_votes_proposal(&self, round: BlockRound) -> DbResult<usize> {
                let tbl = self.open_table(&cons_votes_block::TABLE)?;

                Ok(tbl
                    .range(&(round, PeerIdx::MIN)..=&(round, PeerIdx::MAX))?
                    .count())
            }

            fn get_votes_dummy(&self, round: BlockRound) -> DbResult<Vec<(PeerIdx, Signature)>> {
                let mut sigs = vec![];
                let tbl = self.open_table(&cons_votes_dummy::TABLE)?;

                for kv in tbl.range(&(round, PeerIdx::MIN)..=&(round, PeerIdx::MAX))? {
                    let (k, v) = kv?;

                    sigs.push((k.value().1, v.value()));
                }

                Ok(sigs)
            }

            fn get_votes_proposal(&self, round: BlockRound) -> DbResult<Vec<(PeerIdx, Signature)>> {
                let mut sigs = vec![];
                let tbl = self.open_table(&cons_votes_block::TABLE)?;

                for kv in tbl.range(&(round, PeerIdx::MIN)..=&(round, PeerIdx::MAX))? {
                    let (k, v) = kv?;

                    sigs.push((k.value().1, v.value().sig));
                }

                Ok(sigs)
            }
        }
    };
}
impl_consensus_read_db_ops!(ReadTransaction);
impl_consensus_read_db_ops!(WriteTransactionCtx);

impl ConsensusWriteDbOps for WriteTransactionCtx {
    fn insert_consensus_params(
        &self,
        round: BlockRound,
        params: &ConsensusParams,
    ) -> DbResult<InsertOutcome<ConsensusParamsHash>> {
        let mut tbl_schedule = self.open_table(&cons_params_schedule::TABLE)?;
        let mut tbl = self.open_table(&cons_params::TABLE)?;

        let hash = params.hash();

        if let Some(existing) = tbl_schedule.insert(&round, &hash)?.map(|v| v.value()) {
            return Ok(InsertOutcome::AlreadyPresent(existing));
        }
        tbl.insert(&hash, params)?;
        Ok(InsertOutcome::Inserted)
    }

    fn insert_block_proposal(
        &self,
        round: BlockRound,
        block: BlockHeader,
        payload: &BlockPayloadRaw,
    ) -> DbResult<InsertOutcome<BlockHeader>> {
        assert_eq!(block.round, round);
        debug_assert!(block.payload_hash == payload.hash() && block.payload_len == payload.len());

        let mut tbl_blocks = self.open_table(&cons_blocks_proposals::TABLE)?;
        if let Some(existing) = tbl_blocks.get(&round)?.map(|x| x.value()) {
            // We already inserted this block before
            return Ok(InsertOutcome::AlreadyPresent(existing));
        }

        tbl_blocks.insert(&round, &block)?;

        let mut tbl_blocks_payloads = self.open_table(&cons_blocks_payloads::TABLE)?;
        tbl_blocks_payloads.insert(&block.payload_hash, payload)?;

        let mut tbl_votes_blocks = self.open_table(&cons_votes_block::TABLE)?;

        // Delete any votes that were for a block different than this one
        //
        // This is because we optimistically collect proposal votes even before we have
        // the proposal itself, so once we receive the proposal, we need to throw away
        // any sigs that were incorrect.
        tbl_votes_blocks.retain_in(&(round, PeerIdx::MIN)..&(round, PeerIdx::MAX), |_, vote| {
            vote.inner == block
        })?;

        Ok(InsertOutcome::Inserted)
    }

    fn update_finality_consensus(&self, round: BlockRound) -> DbResult<Option<BlockRound>> {
        let mut tbl = self.open_table(&cons_finality_consensus::TABLE)?;
        let prev = tbl.get(&())?.map(|x| x.value());
        tbl.insert(&(), &round)?;

        Ok(prev)
    }

    fn update_finality_vote(
        &self,
        peer_pubkey: PeerPubkey,
        round: BlockRound,
    ) -> DbResult<Option<BlockRound>> {
        let mut tbl = self.open_table(&cons_finality_votes::TABLE)?;
        let prev = tbl.get(&peer_pubkey)?.map(|x| x.value());
        tbl.insert(&peer_pubkey, &round)?;

        Ok(prev)
    }

    fn insert_notarized_block(
        &self,
        round: BlockRound,
        block: BlockHeader,
        payload: Option<&BlockPayloadRaw>,
    ) -> DbResult<InsertOutcome<BlockHeader>> {
        assert_eq!(block.round, round);
        debug_assert!(
            payload.is_none_or(|payload| block.payload_hash == payload.hash()
                && block.payload_len == payload.len())
        );
        assert!(!block.is_dummy());
        let mut tbl_blocks = self.open_table(&cons_blocks_notarized::TABLE)?;
        if let Some(existing) = tbl_blocks.get(&round)?.map(|x| x.value()) {
            // We already inserted this block before, and since it is notarized, it has to
            // be the same one.
            debug_assert_eq!(existing, block);
            return Ok(InsertOutcome::AlreadyPresent(existing));
        }
        tbl_blocks.insert(&round, &block)?;

        if let Some(payload) = payload {
            let mut tbl_blocks_payloads = self.open_table(&cons_blocks_payloads::TABLE)?;
            tbl_blocks_payloads.insert(&block.payload_hash, payload)?;
        }

        let mut tbl_votes_blocks = self.open_table(&cons_votes_block::TABLE)?;

        // Delete any votes that were for a block different than this one
        //
        // The proposal, or even votes collected before it might have been for a
        // different block.
        tbl_votes_blocks.retain_in(&(round, PeerIdx::MIN)..&(round, PeerIdx::MAX), |_, vote| {
            vote.inner == block
        })?;

        Ok(InsertOutcome::Inserted)
    }

    fn insert_pinned_block(
        &self,
        round: BlockRound,
        hash: BlockHash,
    ) -> DbResult<InsertOutcome<BlockHash>> {
        let mut tbl = self.open_table(&cons_blocks_pinned::TABLE)?;

        if let Some(existing) = tbl.insert(&round, &hash)?.map(|v| v.value()) {
            return Ok(InsertOutcome::AlreadyPresent(existing));
        }
        Ok(InsertOutcome::Inserted)
    }

    fn delete_notarized_block(&self, round: BlockRound) -> DbResult<()> {
        let mut tbl_blocks = self.open_table(&cons_blocks_notarized::TABLE)?;

        tbl_blocks.remove(&round)?;
        // Note: we never remove payloads, they can just hang around, it's fine.

        Ok(())
    }

    fn insert_dummy_vote(
        &self,
        round: BlockRound,
        peerd_idx: PeerIdx,
        vote: Signature,
    ) -> DbResult<InsertOutcome<Signature>> {
        let mut tbl = self.open_table(&cons_votes_dummy::TABLE)?;

        if let Some(existing) = tbl.insert(&(round, peerd_idx), &vote)?.map(|v| v.value()) {
            return Ok(InsertOutcome::AlreadyPresent(existing));
        }
        Ok(InsertOutcome::Inserted)
    }

    fn insert_block_vote(
        &self,
        round: BlockRound,
        peerd_idx: PeerIdx,
        vote: SignedVote,
    ) -> DbResult<InsertOutcome<SignedVote>> {
        let mut tbl = self.open_table(&cons_votes_block::TABLE)?;

        if let Some(existing) = tbl.insert(&(round, peerd_idx), &vote)?.map(|v| v.value()) {
            return Ok(InsertOutcome::AlreadyPresent(existing));
        }
        Ok(InsertOutcome::Inserted)
    }

    fn set_current_round(&self, round: BlockRound) -> DbResult<()> {
        let mut tbl = self.open_table(&cons_current_round::TABLE)?;
        tbl.insert(&(), &round)?;
        Ok(())
    }
}
