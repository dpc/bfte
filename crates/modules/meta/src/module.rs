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
use bfte_module::effect::{CItemEffect, EffectKindExt, ModuleCItemEffect};
use bfte_module::module::IModule;
use bfte_module::module::db::{
    DbResult, DbTxResult, ModuleDatabase, ModuleReadableTransaction, ModuleWriteTransactionCtx,
};
use bfte_util_db::redb_bincode::ReadableTable as _;
use bfte_util_error::{Whatever, WhateverResult};
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
                tbl.range((key, PeerPubkey::MIN)..(key.wrapping_add(1), PeerPubkey::MIN))?
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
            .read_with_expect(|dbtx| self.refresh_consensus_proposals_tx(dbtx))
            .await;

        self.propose_citems_tx.send_replace(proposals);
    }

    pub(crate) fn refresh_consensus_proposals_tx<'dbtx>(
        &self,
        dbtx: &impl ModuleReadableTransaction<'dbtx>,
    ) -> DbResult<Vec<CItemRaw>> {
        let mut proposals = vec![];

        // Get pending proposals
        let pending_tbl = dbtx.open_table(&tables::pending_proposals::TABLE)?;
        for kv in pending_tbl.range(..)? {
            let (key, value) = kv?;
            let citem = MetaCitem::VoteKeyValue {
                key: key.value(),
                value: value.value(),
            };
            proposals.push(citem.encode_to_raw());
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

        // Count votes for this specific value
        let mut vote_count = 0;
        for kv in votes_tbl.range((key, PeerPubkey::MIN)..(key.wrapping_add(1), PeerPubkey::MIN))? {
            let (_key_voter, vote_value) = kv?;
            if vote_value.value() == value {
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

            // Clear all votes for this key since consensus is reached
            votes_tbl.retain(|(vote_key, _), _| *vote_key != key)?;

            // Emit consensus effect
            effects.push((KeyValueConsensusEffect { key, value }).encode());
        }

        Ok(effects)
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
            MetaCitem::VoteKeyValue { key, value } => {
                self.process_citem_vote_key_value(dbtx, peer_pubkey, peer_set, key, value)
            }
        }?;

        // Refresh proposals after processing
        let proposals = self.refresh_consensus_proposals_tx(dbtx)?;
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
        _dbtx: &ModuleWriteTransactionCtx,
        _effects: &[ModuleCItemEffect],
    ) -> DbTxResult<(), Whatever> {
        // Meta module doesn't need to process effects from other modules
        Ok(())
    }
}
