use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use bfte_consensus_core::block::BlockRound;
use bfte_consensus_core::citem::{CItemRaw, InputRaw, OutputRaw};
use bfte_consensus_core::num_peers::ToNumPeers;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_consensus_core::peer_set::PeerSet;
use bfte_consensus_core::ver::ConsensusVersion;
use bfte_db::error::TxSnafu;
use bfte_module::effect::{CItemEffect, EffectKind, EffectKindExt, ModuleCItemEffect};
use bfte_module::module::IModule;
use bfte_module::module::db::{
    DbResult, DbTxResult, ModuleDatabase, ModuleReadableTransaction, ModuleWriteTransactionCtx,
};
use bfte_module_app_consensus::effects::RemovePeerEffect;
use bfte_util_db::redb_bincode::ReadableTable as _;
use bfte_util_error::{Whatever, WhateverResult};
use convi::CastFrom as _;
use snafu::{OptionExt as _, ResultExt as _};
use tokio::sync::watch;

use crate::citem::MetaCitem;
use crate::effects::KeyValueConsensusEffect;
use crate::tables;

pub struct MetaModule {
    #[allow(dead_code)]
    pub(crate) version: ConsensusVersion,
    pub(crate) db: ModuleDatabase,
    #[allow(dead_code)]
    pub(crate) peer_pubkey: Option<PeerPubkey>,
    pub(crate) propose_citems_rx: watch::Receiver<Vec<CItemRaw>>,
    pub(crate) propose_citems_tx: watch::Sender<Vec<CItemRaw>>,
}

impl MetaModule {
    pub fn new(
        version: ConsensusVersion,
        db: ModuleDatabase,
        peer_pubkey: Option<PeerPubkey>,
    ) -> Self {
        let (propose_citems_tx, propose_citems_rx) = watch::channel(vec![]);
        Self {
            version,
            db,
            peer_pubkey,
            propose_citems_rx,
            propose_citems_tx,
        }
    }

    pub(crate) fn init_db_tx(dbtx: &ModuleWriteTransactionCtx) -> DbResult<()> {
        dbtx.open_table(&tables::consensus_values::TABLE)?;
        dbtx.open_table(&tables::key_value_votes::TABLE)?;
        dbtx.open_table(&tables::pending_proposals::TABLE)?;
        Ok(())
    }
    /// Get current agreed consensus values
    pub async fn get_consensus_values(&self) -> BTreeMap<u8, Arc<[u8]>> {
        self.db
            .read_with_expect(|dbtx| {
                let tbl = dbtx.open_table(&tables::consensus_values::TABLE)?;
                tbl.range(..)?
                    .map(|kv| {
                        let (key, value) = kv?;
                        Ok((key.value(), value.value()))
                    })
                    .collect()
            })
            .await
    }

    /// Get current votes for a specific key
    pub async fn get_votes_for_key(&self, key: u8) -> BTreeMap<PeerPubkey, Arc<[u8]>> {
        self.db
            .read_with_expect(|dbtx| {
                let tbl = dbtx.open_table(&tables::key_value_votes::TABLE)?;
                tbl.range((key, PeerPubkey::ZERO)..=(key, PeerPubkey::MAX))?
                    .map(|kv| {
                        let (key_voter, value) = kv?;
                        let (_, voter) = key_voter.value();
                        Ok((voter, value.value()))
                    })
                    .collect()
            })
            .await
    }

    /// Propose a value for a key
    pub async fn propose_key_value(&self, key: u8, value: Arc<[u8]>) -> WhateverResult<()> {
        self.db
            .write_with_expect(|dbtx| {
                let mut tbl = dbtx.open_table(&tables::pending_proposals::TABLE)?;
                tbl.insert(&key, &value)?;
                Ok(())
            })
            .await;
        self.refresh_consensus_proposals().await;
        Ok(())
    }

    pub(crate) async fn refresh_consensus_proposals(&self) {
        let proposals = self
            .db
            .read_with_expect(|dbtx| self.refresh_consensus_proposals_dbtx(dbtx))
            .await;

        self.propose_citems_tx.send_replace(proposals);
    }

    pub(crate) fn refresh_consensus_proposals_dbtx<'dbtx>(
        &self,
        dbtx: &impl ModuleReadableTransaction<'dbtx>,
    ) -> DbResult<Vec<CItemRaw>> {
        let mut proposals = vec![];

        // Get pending proposals
        let pending_tbl = dbtx.open_table(&tables::pending_proposals::TABLE)?;
        let votes_tbl = dbtx.open_table(&tables::key_value_votes::TABLE)?;

        for kv in pending_tbl.range(..)? {
            let (key, value) = kv?;
            let key = key.value();
            let value = value.value();

            // Check if we already have the same vote for this key
            let should_propose = if let Some(peer_pubkey) = &self.peer_pubkey {
                match votes_tbl.get(&(key, *peer_pubkey))? {
                    Some(existing_vote) => existing_vote.value() != value,
                    None => true, // No existing vote, so we should propose
                }
            } else {
                true // No peer pubkey, so always propose
            };

            if should_propose {
                // Check if any other peer already voted for the same value
                let mut found_matching_peer = None;
                for vote_kv in votes_tbl.range((key, PeerPubkey::ZERO)..=(key, PeerPubkey::MAX))? {
                    let (key_and_peer, vote_value) = vote_kv?;
                    let (vote_key, vote_peer) = key_and_peer.value();
                    assert_eq!(key, vote_key);
                    let vote_value = vote_value.value();
                    if vote_value == value {
                        found_matching_peer = Some(vote_peer);
                        break;
                    }
                }

                let citem = if let Some(matching_peer) = found_matching_peer {
                    // Approve existing vote instead of proposing the same value again
                    MetaCitem::ApproveVote {
                        key,
                        peer_pubkey: matching_peer,
                    }
                } else {
                    // No matching vote found, propose the value
                    MetaCitem::ProposeValue { key, value }
                };

                proposals.push(citem.encode_to_raw());
            }
        }

        Ok(proposals)
    }

    fn process_citem_vote_key_value(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        voter_pubkey: PeerPubkey,
        peer_set: &PeerSet,
        key: u8,
        value: Arc<[u8]>,
    ) -> DbTxResult<Vec<CItemEffect>, Whatever> {
        let mut effects = vec![];

        // Open the votes table
        let mut votes_tbl = dbtx.open_table(&tables::key_value_votes::TABLE)?;

        // Check if this vote creates a change
        let existing_vote = votes_tbl.get(&(key, voter_pubkey))?.map(|v| v.value());

        if existing_vote.as_ref() == Some(&value) {
            // Vote already recorded, no change needed
            return Ok(vec![]);
        }

        // Record the vote
        votes_tbl.insert(&(key, voter_pubkey), &value)?;

        // Count votes for this specific value from peers in the current peer set
        let mut vote_count = 0;
        for kv in votes_tbl.range((key, PeerPubkey::ZERO)..=(key, PeerPubkey::MAX))? {
            let (key_and_peer, vote_value) = kv?;

            let (_, voter) = key_and_peer.value();
            if vote_value.value() == value && peer_set.contains(&voter) {
                vote_count += 1;
            }
        }

        // Check if we've reached threshold
        let threshold = peer_set.to_num_peers().threshold();
        if vote_count >= threshold {
            // Threshold reached - set consensus value and emit effect

            // Remove pending proposal for this key if it exists
            {
                let mut pending_tbl = dbtx.open_table(&tables::pending_proposals::TABLE)?;
                pending_tbl.remove(&key)?;
            }

            // Set the consensus value
            {
                let mut consensus_tbl = dbtx.open_table(&tables::consensus_values::TABLE)?;
                consensus_tbl.insert(&key, &value)?;
            }

            // Clear votes for this key from current peer set since consensus is reached
            votes_tbl
                .retain(|(vote_key, voter), _| *vote_key != key || !peer_set.contains(voter))?;

            // Emit consensus effect
            effects.push((KeyValueConsensusEffect { key, value }).encode());
        }

        Ok(effects)
    }

    fn process_citem_approve_vote(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        voter_pubkey: PeerPubkey,
        peer_set: &PeerSet,
        key: u8,
        approved_peer: PeerPubkey,
    ) -> DbTxResult<Vec<CItemEffect>, Whatever> {
        // Look up the value that the approved peer voted for
        let votes_tbl = dbtx.open_table(&tables::key_value_votes::TABLE)?;

        match votes_tbl.get(&(key, approved_peer))? {
            Some(approved_value) => {
                // Cast our vote for the same value as the approved peer
                let value = approved_value.value();
                self.process_citem_vote_key_value(dbtx, voter_pubkey, peer_set, key, value)
            }
            None => {
                // The approved peer hasn't voted for this key, which is invalid
                // Return empty effects (essentially ignore this vote)
                None.whatever_context("Invalid peer_pubkey")
                    .context(TxSnafu)?
            }
        }
    }

    /// Recheck all existing votes to see if any can now reach consensus with
    /// the updated peer set
    fn recheck_consensus_after_peer_removal(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        peer_set: &PeerSet,
    ) -> DbTxResult<(), Whatever> {
        let votes_tbl = dbtx.open_table(&tables::key_value_votes::TABLE)?;
        let consensus_tbl = dbtx.open_table(&tables::consensus_values::TABLE)?;

        // Collect all unique keys that have votes
        let mut keys_with_votes = std::collections::BTreeSet::new();
        for kv in votes_tbl.range(..)? {
            let (key_and_peer, _) = kv?;
            let (key, _) = key_and_peer.value();
            keys_with_votes.insert(key);
        }

        // For each key, check if any value can now reach consensus
        for key in keys_with_votes {
            // Skip if consensus already exists for this key
            if consensus_tbl.get(&key)?.is_some() {
                continue;
            }

            // Count votes for each value from current peer set members
            let mut value_counts: BTreeMap<Arc<[u8]>, u32> = BTreeMap::new();
            for kv in votes_tbl.range((key, PeerPubkey::ZERO)..=(key, PeerPubkey::MAX))? {
                let (key_and_peer, vote_value) = kv?;
                let (_, voter) = key_and_peer.value();

                if peer_set.contains(&voter) {
                    let value = vote_value.value();
                    *value_counts.entry(value).or_insert(0) += 1;
                }
            }

            // Check if any value has reached threshold
            let threshold = peer_set.to_num_peers().threshold();
            for (value, count) in value_counts {
                if usize::cast_from(count) >= threshold {
                    // Consensus reached! Set the value
                    let mut consensus_tbl = dbtx.open_table(&tables::consensus_values::TABLE)?;
                    consensus_tbl.insert(&key, &value)?;

                    // Remove pending proposal for this key if it exists
                    let mut pending_tbl = dbtx.open_table(&tables::pending_proposals::TABLE)?;
                    pending_tbl.remove(&key)?;

                    // Clear votes for this key from current peer set since consensus is reached
                    let mut votes_tbl = dbtx.open_table(&tables::key_value_votes::TABLE)?;
                    votes_tbl.retain(|(vote_key, voter), _| {
                        *vote_key != key || !peer_set.contains(voter)
                    })?;

                    // Note: We don't emit KeyValueConsensusEffect here because process_effects
                    // is not designed to emit effects. The consensus is recorded in the database
                    // and can be observed through get_consensus_values().

                    break; // Only one value can reach consensus per key
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl IModule for MetaModule {
    async fn propose_citems_rx(&self) -> watch::Receiver<Vec<CItemRaw>> {
        self.refresh_consensus_proposals().await;
        self.propose_citems_rx.clone()
    }

    fn process_citem(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        _round: BlockRound,
        peer_pubkey: PeerPubkey,
        peer_set: &PeerSet,
        citem: &CItemRaw,
    ) -> DbTxResult<Vec<CItemEffect>, Whatever> {
        assert!(peer_set.contains(&peer_pubkey));
        let citem = MetaCitem::decode_from_raw(citem).context(TxSnafu)?;

        let res = match citem {
            MetaCitem::ProposeValue { key, value } => {
                self.process_citem_vote_key_value(dbtx, peer_pubkey, peer_set, key, value)
            }
            MetaCitem::ApproveVote {
                key,
                peer_pubkey: approved_peer,
            } => self.process_citem_approve_vote(dbtx, peer_pubkey, peer_set, key, approved_peer),
        }?;

        // Refresh proposals after processing
        let proposals = self.refresh_consensus_proposals_dbtx(dbtx)?;
        let tx = self.propose_citems_tx.clone();

        dbtx.on_commit(move || {
            tx.send_replace(proposals);
        });

        Ok(res)
    }

    fn process_input(
        &self,
        _dbtx: &ModuleWriteTransactionCtx,
        _input: &InputRaw,
    ) -> DbTxResult<Vec<CItemEffect>, Whatever> {
        None.whatever_context("Meta module does not support any inputs")
            .context(TxSnafu)?
    }

    fn process_output(
        &self,
        _dbtx: &ModuleWriteTransactionCtx,
        _output: &OutputRaw,
    ) -> DbTxResult<Vec<CItemEffect>, Whatever> {
        None.whatever_context("Meta module does not support any outputs")
            .context(TxSnafu)?
    }

    fn process_effects(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        peer_set: &PeerSet,
        effects: &[ModuleCItemEffect],
    ) -> DbTxResult<(), Whatever> {
        for effect in effects {
            // Only process effects from the app-consensus module (peer management)
            if effect.module_kind() != bfte_module_app_consensus::KIND {
                continue;
            }

            // Handle RemovePeerEffect
            if effect.inner().effect_id == RemovePeerEffect::EFFECT_ID {
                // A peer was removed, recheck if existing votes can now reach consensus
                self.recheck_consensus_after_peer_removal(dbtx, peer_set)?;
            }
        }

        Ok(())
    }
}
