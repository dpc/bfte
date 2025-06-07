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
            whatever!("Cannot remove the last peer from the consensus")
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

        let pending_add_vote = {
            let tbl = dbtx.open_table(&tables::pending_add_peer_vote::TABLE)?;
            tbl.get(&())?.map(|v| v.value())
        };

        if let Some(pending_peer) = pending_add_vote {
            let current_vote = {
                let tbl = dbtx.open_table(&tables::add_peer_votes::TABLE)?;
                tbl.get(&peer_pubkey)?.map(|v| v.value())
            };

            if current_vote != Some(pending_peer) {
                let citem = AppConsensusCitem::VoteAddPeer(pending_peer);
                proposals.push(citem.encode_to_raw());
            }
        }

        let pending_remove_vote = {
            let tbl = dbtx.open_table(&tables::pending_remove_peer_vote::TABLE)?;
            tbl.get(&())?.map(|v| v.value())
        };

        if let Some(pending_peer) = pending_remove_vote {
            let current_vote = {
                let tbl = dbtx.open_table(&tables::remove_peer_votes::TABLE)?;
                tbl.get(&peer_pubkey)?.map(|v| v.value())
            };

            if current_vote != Some(pending_peer) {
                let citem = AppConsensusCitem::VoteRemovePeer(pending_peer);
                proposals.push(citem.encode_to_raw());
            }
        }

        Ok(proposals)
    }

    /// Get the current peer set within a transaction context
    fn get_peer_set_in_tx(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
    ) -> DbTxResult<PeerSet, Whatever> {
        let tbl = dbtx.open_table(&tables::peers::TABLE)?;
        let peers = tbl
            .range(..)?
            .map(|kv| {
                let (k, _) = kv.map_err(bfte_db::error::DbTxError::from)?;
                Ok(k.value())
            })
            .collect::<Result<Vec<_>, bfte_db::error::DbTxError<_>>>()?;
        Ok(peers.into())
    }

    fn process_citem_vote_add_peer(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        voter_pubkey: PeerPubkey,
        peer_set: &PeerSet,
        peer_to_add: PeerPubkey,
    ) -> DbTxResult<Vec<CItemEffect>, Whatever> {
        // Verify the voter is actually in the current peer set
        if !peer_set.iter().any(|&p| p == voter_pubkey) {
            None.whatever_context("Voter {voter_pubkey} is not in the current peer set")
                .context(TxSnafu)?;
        }

        // Open the votes table for reading and writing
        let mut tbl = dbtx.open_table(&tables::add_peer_votes::TABLE)?;

        // Check if this vote creates a change
        let existing_vote = tbl.get(&voter_pubkey)?.map(|v| v.value());

        if existing_vote == Some(peer_to_add) {
            // Vote already recorded, no change needed
            return Ok(vec![]);
        }

        // Record the vote directly in the database
        tbl.insert(&voter_pubkey, &peer_to_add)?;

        // If this vote is from ourselves, clear the pending vote to stop proposing the
        // same citem
        if Some(voter_pubkey) == self.peer_pubkey {
            let mut pending_tbl = dbtx.open_table(&tables::pending_add_peer_vote::TABLE)?;
            pending_tbl.remove(&())?;
        }

        // Count votes for the peer_to_add from all peer set members
        let mut votes_for_candidate = 0;
        for peer in peer_set.iter() {
            match tbl.get(peer)? {
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

            // Clear all votes for adding this peer since it's now being added
            // (we already have the table open, so we can reuse it)
            let peers_to_clear: Vec<PeerPubkey> = tbl
                .range(..)?
                .filter_map(|kv| {
                    let (voter, voted_for) = kv.ok()?;
                    if voted_for.value() == peer_to_add {
                        Some(voter.value())
                    } else {
                        None
                    }
                })
                .collect();

            for voter in peers_to_clear {
                tbl.remove(&voter)?;
            }

            // Insert the peer into the peers table
            {
                let mut peers_tbl = dbtx.open_table(&tables::peers::TABLE)?;
                peers_tbl.insert(&peer_to_add, &())?;
            }

            let add_peer_effect = AddPeerEffect { peer: peer_to_add };
            effects.push(EffectKindExt::encode(&add_peer_effect));

            // Get the updated peer set and emit PeerSetChange effect
            let updated_peer_set = self.get_peer_set_in_tx(dbtx)?;
            let peer_set_change_effect = ConsensusParamsChange {
                peer_set: updated_peer_set,
            };
            effects.push(EffectKindExt::encode(&peer_set_change_effect));
        }

        Ok(effects)
    }

    fn process_citem_vote_remove_peer(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        voter_pubkey: PeerPubkey,
        peer_set: &PeerSet,
        peer_to_remove: PeerPubkey,
    ) -> DbTxResult<Vec<CItemEffect>, Whatever> {
        // Verify the voter is actually in the current peer set
        if !peer_set.iter().any(|&p| p == voter_pubkey) {
            None.whatever_context("Voter {voter_pubkey} is not in the current peer set")
                .context(TxSnafu)?;
        }

        // Check if this would be the last peer - don't allow removing the last peer
        if peer_set.len() == 1 {
            None.whatever_context("Cannot remove the last peer from the consensus")
                .context(TxSnafu)?;
        }

        // Check if the peer to remove is actually in the peer set
        if !peer_set.iter().any(|&p| p == peer_to_remove) {
            None.whatever_context("Peer to remove is not in the current peer set")
                .context(TxSnafu)?;
        }

        // Open the votes table for reading and writing
        let mut tbl = dbtx.open_table(&tables::remove_peer_votes::TABLE)?;

        // Check if this vote creates a change
        let existing_vote = tbl.get(&voter_pubkey)?.map(|v| v.value());

        if existing_vote == Some(peer_to_remove) {
            // Vote already recorded, no change needed
            return Ok(vec![]);
        }

        // Record the vote directly in the database
        tbl.insert(&voter_pubkey, &peer_to_remove)?;

        // If this vote is from ourselves, clear the pending vote to stop proposing the
        // same citem
        if Some(voter_pubkey) == self.peer_pubkey {
            let mut pending_tbl = dbtx.open_table(&tables::pending_remove_peer_vote::TABLE)?;
            pending_tbl.remove(&())?;
        }

        // Count votes for the peer_to_remove from all peer set members
        let mut votes_for_removal = 0;
        for peer in peer_set.iter() {
            match tbl.get(peer)? {
                Some(vote) if vote.value() == peer_to_remove => {
                    votes_for_removal += 1;
                }
                _ => {} // No vote or vote for different peer
            }
        }

        // Check if threshold is reached
        let num_peers = peer_set.to_num_peers();
        let threshold = num_peers.threshold();

        let mut effects = vec![];
        if votes_for_removal >= threshold {
            // Threshold reached - remove the peer immediately and emit effect

            // Clear all votes for removing this peer since it's now being removed
            // (we already have the table open, so we can reuse it)
            let peers_to_clear: Vec<PeerPubkey> = tbl
                .range(..)?
                .filter_map(|kv| {
                    let (voter, voted_for) = kv.ok()?;
                    if voted_for.value() == peer_to_remove {
                        Some(voter.value())
                    } else {
                        None
                    }
                })
                .collect();

            for voter in peers_to_clear {
                tbl.remove(&voter)?;
            }

            // Remove the peer from the peers table
            {
                let mut peers_tbl = dbtx.open_table(&tables::peers::TABLE)?;
                peers_tbl.remove(&peer_to_remove)?;
            }

            let remove_peer_effect = RemovePeerEffect {
                peer: peer_to_remove,
            };
            effects.push(EffectKindExt::encode(&remove_peer_effect));

            // Get the updated peer set and emit PeerSetChange effect
            let updated_peer_set = self.get_peer_set_in_tx(dbtx)?;
            let peer_set_change_effect = ConsensusParamsChange {
                peer_set: updated_peer_set,
            };
            effects.push(EffectKindExt::encode(&peer_set_change_effect));
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
