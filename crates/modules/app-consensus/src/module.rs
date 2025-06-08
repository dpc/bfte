use std::collections::BTreeMap;

use async_trait::async_trait;
use bfte_consensus_core::block::BlockRound;
use bfte_consensus_core::citem::{CItemRaw, InputRaw, OutputRaw};
use bfte_consensus_core::module::ModuleId;
use bfte_consensus_core::num_peers::ToNumPeers;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_consensus_core::peer_set::PeerSet;
use bfte_consensus_core::ver::ConsensusVersion;
use bfte_db::error::TxSnafu;
use bfte_module::effect::{CItemEffect, EffectKindExt, ModuleCItemEffect};
use bfte_module::module::IModule;
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::db::{
    DbResult, DbTxResult, ModuleDatabase, ModuleReadableTransaction, ModuleWriteTransactionCtx,
};
use bfte_util_db::redb_bincode::ReadableTable as _;
use bfte_util_error::{Whatever, WhateverResult};
use snafu::{OptionExt as _, ResultExt as _, whatever};
use tokio::sync::watch;

use crate::citem::AppConsensusCitem;
use crate::effects::{AddPeerEffect, ConsensusParamsChange, RemovePeerEffect};
use crate::tables;

pub struct AppConsensusModule {
    #[allow(dead_code)]
    pub(crate) version: ConsensusVersion,
    pub(crate) db: ModuleDatabase,
    pub(crate) peer_pubkey: Option<PeerPubkey>,
    pub(crate) propose_citems_rx: watch::Receiver<Vec<CItemRaw>>,
    pub(crate) propose_citems_tx: watch::Sender<Vec<CItemRaw>>,
}

impl AppConsensusModule {
    pub(crate) async fn get_module_configs_static(
        db: &ModuleDatabase,
    ) -> BTreeMap<ModuleId, ModuleConfig> {
        db.read_with_expect(|dbtx| {
            let tbl = dbtx.open_table(&tables::modules_configs::TABLE)?;

            tbl.range(..)?
                .map(|kv| {
                    let (k, v) = kv?;

                    let module_id = k.value();
                    let value = v.value();
                    Ok((
                        module_id,
                        ModuleConfig {
                            kind: value.kind,
                            version: value.version,
                            params: value.params,
                        },
                    ))
                })
                .collect()
        })
        .await
    }
    pub async fn get_modules_configs(&self) -> BTreeMap<ModuleId, ModuleConfig> {
        Self::get_module_configs_static(&self.db).await
    }

    pub async fn get_peer_set(&self) -> PeerSet {
        self.db
            .read_with_expect(|dbtx| {
                let tbl = dbtx.open_table(&tables::peers::TABLE)?;

                tbl.range(..)?
                    .map(|kv| {
                        let (k, _) = kv?;
                        Ok(k.value())
                    })
                    .collect()
            })
            .await
    }

    pub async fn set_pending_add_peer_vote(&self, peer_to_add: PeerPubkey) -> WhateverResult<()> {
        if self.peer_pubkey.is_none() {
            whatever!("Cannot cast votes: note a voting peer")
        }

        let peer_already_exists = self
            .db
            .read_with_expect(|dbtx| {
                let tbl = dbtx.open_table(&tables::peers::TABLE)?;
                Ok(tbl.get(&peer_to_add)?.is_some())
            })
            .await;

        if peer_already_exists {
            return Ok(());
        }

        self.db
            .write_with_expect(|dbtx| {
                let mut tbl = dbtx.open_table(&tables::pending_add_peer_vote::TABLE)?;
                tbl.insert(&(), &peer_to_add)?;
                Ok(())
            })
            .await;

        self.refresh_consensus_proposals().await;

        Ok(())
    }

    pub async fn set_pending_remove_peer_vote(
        &self,
        peer_to_remove: PeerPubkey,
    ) -> WhateverResult<()> {
        if self.peer_pubkey.is_none() {
            whatever!("Cannot cast votes: not a voting peer")
        }
        let peer_exists = self
            .db
            .read_with_expect(|dbtx| {
                let tbl = dbtx.open_table(&tables::peers::TABLE)?;
                Ok(tbl.get(&peer_to_remove)?.is_some())
            })
            .await;

        if !peer_exists {
            return Ok(());
        }
        let peer_set = self.get_peer_set().await;
        if peer_set.len() == 1 {
            whatever!(
                "Cannot remove the last peer from the consensus: {}",
                peer_to_remove
            );
        }
        self.db
            .write_with_expect(|dbtx| {
                let mut tbl = dbtx.open_table(&tables::pending_remove_peer_vote::TABLE)?;
                tbl.insert(&(), &peer_to_remove)?;
                Ok(())
            })
            .await;
        self.refresh_consensus_proposals().await;
        Ok(())
    }

    pub async fn get_add_peer_votes(&self) -> BTreeMap<PeerPubkey, PeerPubkey> {
        self.db
            .read_with_expect(|dbtx| {
                let tbl = dbtx.open_table(&tables::add_peer_votes::TABLE)?;
                tbl.range(..)?
                    .map(|kv| {
                        let (voter, voted_for) = kv?;
                        Ok((voter.value(), voted_for.value()))
                    })
                    .collect()
            })
            .await
    }

    pub async fn get_remove_peer_votes(&self) -> BTreeMap<PeerPubkey, PeerPubkey> {
        self.db
            .read_with_expect(|dbtx| {
                let tbl = dbtx.open_table(&tables::remove_peer_votes::TABLE)?;
                tbl.range(..)?
                    .map(|kv| {
                        let (voter, voted_for) = kv?;
                        Ok((voter.value(), voted_for.value()))
                    })
                    .collect()
            })
            .await
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
        let mut proposals = Vec::new();

        let Some(peer_pubkey) = self.peer_pubkey else {
            return Ok(proposals);
        };

        let peer_set = self.get_peer_set_tx(dbtx)?;

        let pending_add_vote = {
            let tbl = dbtx.open_table(&tables::pending_add_peer_vote::TABLE)?;
            tbl.get(&())?.map(|v| v.value())
        };

        if let Some(pending_peer) = pending_add_vote {
            if !peer_set.contains(&pending_peer) {
                let current_vote = {
                    let tbl = dbtx.open_table(&tables::add_peer_votes::TABLE)?;
                    tbl.get(&peer_pubkey)?.map(|v| v.value())
                };

                if current_vote != Some(pending_peer) {
                    let citem = AppConsensusCitem::VoteAddPeer(pending_peer);
                    proposals.push(citem.encode_to_raw());
                }
            }
        }

        let pending_remove_vote = {
            let tbl = dbtx.open_table(&tables::pending_remove_peer_vote::TABLE)?;
            tbl.get(&())?.map(|v| v.value())
        };

        if let Some(pending_peer) = pending_remove_vote {
            if peer_set.contains(&pending_peer) {
                let current_vote = {
                    let tbl = dbtx.open_table(&tables::remove_peer_votes::TABLE)?;
                    tbl.get(&peer_pubkey)?.map(|v| v.value())
                };

                if current_vote != Some(pending_peer) {
                    let citem = AppConsensusCitem::VoteRemovePeer(pending_peer);
                    proposals.push(citem.encode_to_raw());
                }
            }
        }

        Ok(proposals)
    }

    /// Get the current peer set within a read transaction context
    fn get_peer_set_tx<'dbtx>(
        &self,
        dbtx: &impl ModuleReadableTransaction<'dbtx>,
    ) -> DbResult<PeerSet> {
        let tbl = dbtx.open_table(&tables::peers::TABLE)?;
        tbl.range(..)?
            .map(|kv| {
                let (k, _) = kv?;
                Ok(k.value())
            })
            .collect()
    }

    fn process_citem_vote_add_peer(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        voter_pubkey: PeerPubkey,
        peer_set: &PeerSet,
        peer_to_add: PeerPubkey,
    ) -> DbTxResult<Vec<CItemEffect>, Whatever> {
        {
            // Changes being voted on, are to be made on the latest (possibly not yet
            // effective) peer set
            let latest_peer_set = self.get_peer_set_tx(dbtx)?;

            // Check if the peer to remove is actually in the peer set
            if latest_peer_set.contains(&peer_to_add) {
                None.whatever_context("Peer already part of the peer set")
                    .context(TxSnafu)?;
            }
        }

        // Open the votes table for reading and writing
        let mut add_peer_votes_tbl = dbtx.open_table(&tables::add_peer_votes::TABLE)?;

        // Record the vote directly in the database
        add_peer_votes_tbl.insert(&voter_pubkey, &peer_to_add)?;

        // If this vote is from ourselves, clear the pending vote to stop proposing the
        // same citem
        if Some(voter_pubkey) == self.peer_pubkey {
            dbtx.open_table(&tables::pending_add_peer_vote::TABLE)?
                .remove(&())?;
        }

        // Count votes for the peer_to_add from all peer set members
        let mut votes_for_candidate = 0;
        for peer in peer_set.iter() {
            match add_peer_votes_tbl.get(peer)? {
                Some(vote) if vote.value() == peer_to_add => {
                    votes_for_candidate += 1;
                }
                _ => {} // No vote or vote for different candidate
            }
        }

        // Check if threshold is reached
        let num_peers = peer_set.to_num_peers();
        let threshold = num_peers.threshold();

        let mut effects = vec![];
        if votes_for_candidate >= threshold {
            // Threshold reached - add the peer immediately and emit effect

            add_peer_votes_tbl.retain(|_k, vote| *vote != peer_to_add)?;

            // Insert the peer into the peers table
            {
                let mut peers_tbl = dbtx.open_table(&tables::peers::TABLE)?;
                peers_tbl.insert(&peer_to_add, &())?;
            }

            // Get the updated peer set and emit PeerSetChange effect
            let updated_peer_set = self.get_peer_set_tx(dbtx)?;

            effects.push((AddPeerEffect { peer: peer_to_add }).encode());
            effects.push(
                (ConsensusParamsChange {
                    peer_set: updated_peer_set,
                })
                .encode(),
            );
        }

        Ok(effects)
    }

    fn process_citem_vote_remove_peer(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        voter_pubkey: PeerPubkey,
        cur_effective_peer_set: &PeerSet,
        peer_to_remove: PeerPubkey,
    ) -> DbTxResult<Vec<CItemEffect>, Whatever> {
        {
            // Changes being voted on, are to be made on the latest (possibly not yet
            // effective) peer set
            let latest_peer_set = self.get_peer_set_tx(dbtx)?;

            // Check if the peer to remove is actually in the peer set
            if !latest_peer_set.contains(&peer_to_remove) {
                None.whatever_context("Peer to remove is not in the latest peer set")
                    .context(TxSnafu)?;
            }

            // Check if this would be the last peer - don't allow removing the last peer
            if latest_peer_set.len() == 1 {
                None.whatever_context("Cannot remove the last peer from the consensus")
                    .context(TxSnafu)?;
            }
        }

        // Open the votes table for reading and writing
        let mut remove_peer_votes_tbl = dbtx.open_table(&tables::remove_peer_votes::TABLE)?;

        // Check if this vote creates a change
        let existing_vote = remove_peer_votes_tbl.get(&voter_pubkey)?.map(|v| v.value());

        if existing_vote == Some(peer_to_remove) {
            // Vote already recorded, no change needed
            return Ok(vec![]);
        }

        // Record the vote directly in the database
        remove_peer_votes_tbl.insert(&voter_pubkey, &peer_to_remove)?;

        // If this vote is from ourselves, clear the pending vote to stop proposing the
        // same citem
        if Some(voter_pubkey) == self.peer_pubkey {
            dbtx.open_table(&tables::pending_remove_peer_vote::TABLE)?
                .remove(&())?;
        }

        // Count votes for the peer_to_remove from all peer set members
        let mut votes_for_removal = 0;
        for peer in cur_effective_peer_set.iter() {
            match remove_peer_votes_tbl.get(peer)? {
                Some(vote) if vote.value() == peer_to_remove => {
                    votes_for_removal += 1;
                }
                _ => {} // No vote or vote for different peer
            }
        }

        // Check if threshold is reached
        let threshold = cur_effective_peer_set.to_num_peers().threshold();

        let mut effects = vec![];
        if votes_for_removal >= threshold {
            // Threshold reached - remove the peer immediately and emit effect

            // Remove the peer from the peers table
            {
                let mut peers_tbl = dbtx.open_table(&tables::peers::TABLE)?;
                peers_tbl.remove(&peer_to_remove)?;
            }

            // Get the updated peer set and emit PeerSetChange effect
            let updated_peer_set = self.get_peer_set_tx(dbtx)?;

            // Clear votes that don't make sense anymore
            remove_peer_votes_tbl
                .retain(|k, vote| updated_peer_set.contains(k) && *vote != peer_to_remove)?;

            dbtx.open_table(&tables::add_peer_votes::TABLE)?
                .retain(|k, _vote| updated_peer_set.contains(k))?;

            effects.push(
                (RemovePeerEffect {
                    peer: peer_to_remove,
                })
                .encode(),
            );

            effects.push(
                (ConsensusParamsChange {
                    peer_set: updated_peer_set,
                })
                .encode(),
            );
        }

        Ok(effects)
    }

    pub fn init_db_tx(dbtx: &ModuleWriteTransactionCtx) -> DbResult<()> {
        dbtx.open_table(&tables::modules_configs::TABLE)?;
        dbtx.open_table(&tables::peers::TABLE)?;
        dbtx.open_table(&tables::add_peer_votes::TABLE)?;
        dbtx.open_table(&tables::remove_peer_votes::TABLE)?;
        dbtx.open_table(&tables::pending_add_peer_vote::TABLE)?;
        dbtx.open_table(&tables::pending_remove_peer_vote::TABLE)?;
        Ok(())
    }
}

#[async_trait]
impl IModule for AppConsensusModule {
    fn display_name(&self) -> &'static str {
        "App Consensus"
    }

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
        let citem = AppConsensusCitem::decode_from_raw(citem).context(TxSnafu)?;

        let res = match citem {
            AppConsensusCitem::VoteAddPeer(peer_to_add) => {
                self.process_citem_vote_add_peer(dbtx, peer_pubkey, peer_set, peer_to_add)
            }
            AppConsensusCitem::VoteRemovePeer(peer_to_remove) => {
                self.process_citem_vote_remove_peer(dbtx, peer_pubkey, peer_set, peer_to_remove)
            }
        }?;

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
        None.whatever_context("Module does not support any inputs")
            .context(TxSnafu)?
    }

    fn process_output(
        &self,
        _dbtx: &ModuleWriteTransactionCtx,
        _output: &OutputRaw,
    ) -> DbTxResult<Vec<CItemEffect>, Whatever> {
        None.whatever_context("Module does not support any outputs")
            .context(TxSnafu)?
    }

    fn process_effects(
        &self,
        _dbtx: &ModuleWriteTransactionCtx,
        _effects: &[ModuleCItemEffect],
    ) -> DbTxResult<(), Whatever> {
        Ok(())
    }
}
