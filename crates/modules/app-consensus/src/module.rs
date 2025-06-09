use std::collections::BTreeMap;

use async_trait::async_trait;
use bfte_consensus_core::block::BlockRound;
use bfte_consensus_core::citem::{CItemRaw, InputRaw, OutputRaw};
use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_consensus_core::num_peers::ToNumPeers;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_consensus_core::peer_set::PeerSet;
use bfte_consensus_core::ver::{ConsensusVersion, ConsensusVersionMinor};
use bfte_db::error::TxSnafu;
use bfte_module::effect::{CItemEffect, EffectKindExt, ModuleCItemEffect};
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::db::{
    DbResult, DbTxResult, ModuleDatabase, ModuleReadableTransaction, ModuleWriteTransactionCtx,
};
use bfte_module::module::{IModule, ModuleSupportedConsensusVersions};
use bfte_util_db::redb_bincode::ReadableTable as _;
use bfte_util_error::{Whatever, WhateverResult};
use snafu::{OptionExt as _, ResultExt as _, whatever};
use tokio::sync::watch;

use crate::citem::AppConsensusCitem;
use crate::effects::{
    AddPeerEffect, ConsensusParamsChange, ModuleVersionUpgrade, RemovePeerEffect,
};
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

        // Handle pending module version votes
        {
            let pending_votes_tbl =
                dbtx.open_table(&tables::pending_modules_versions_votes::TABLE)?;
            let current_votes_tbl = dbtx.open_table(&tables::modules_versions_votes::TABLE)?;

            for kv in pending_votes_tbl.range(..)? {
                let (module_id, pending_minor_version) = kv?;
                let module_id = module_id.value();
                let pending_minor_version = pending_minor_version.value();

                let current_vote = current_votes_tbl
                    .get(&(peer_pubkey, module_id))?
                    .map(|v| v.value().minor());

                if current_vote != Some(pending_minor_version) {
                    let citem = AppConsensusCitem::VoteModuleVersion {
                        module_id,
                        minor_consensus_version: pending_minor_version,
                    };
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

    /// Check for module version upgrades and append effects for any upgrades
    ///
    /// Takes tables as arguments to avoid accidentally trying to open
    /// already opened tables.
    fn check_module_version_upgrades(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        modules_configs_tbl: &mut tables::modules_configs::Table,
        versions_votes_tbl: &mut tables::modules_versions_votes::Table,
        peer_set: &PeerSet,
        effects: &mut Vec<CItemEffect>,
    ) -> DbTxResult<(), Whatever> {
        // Collect all modules that need upgrades first
        let mut upgrades = Vec::new();

        for kv in modules_configs_tbl.range(..)? {
            let (module_id, current_config) = kv?;
            let module_id = module_id.value();
            let current_config = current_config.value();

            // Find the minimum version across all peers in the peer set for this module
            let mut min_minor_version: Option<ConsensusVersionMinor> = None;
            for peer in peer_set.iter() {
                let peer_vote = versions_votes_tbl
                    .get(&(*peer, module_id))?
                    .map(|v| v.value().minor())
                    .unwrap_or_else(|| ConsensusVersionMinor::new(0)); // Default to 0 if missing

                min_minor_version = Some(match min_minor_version {
                    None => peer_vote,
                    Some(current_min) => std::cmp::min(current_min, peer_vote),
                });
            }

            if let Some(agreed_minor) = min_minor_version {
                // Check if the agreed version is higher than current
                if agreed_minor > current_config.version.minor() {
                    let old_version = current_config.version;
                    let new_agreed_version =
                        ConsensusVersion::new(current_config.version.major(), agreed_minor);

                    upgrades.push((module_id, current_config, old_version, new_agreed_version));
                }
            }
        }

        // Apply the upgrades and emit effects
        if !upgrades.is_empty() {
            let mut modules_configs_tbl = dbtx.open_table(&tables::modules_configs::TABLE)?;

            for (module_id, current_config, old_version, new_agreed_version) in upgrades {
                // Update the module configuration
                let updated_config = ModuleConfig {
                    kind: current_config.kind,
                    version: new_agreed_version,
                    params: current_config.params.clone(),
                };
                modules_configs_tbl.insert(&module_id, &updated_config)?;

                // Emit module version upgrade effect
                effects.push(
                    (ModuleVersionUpgrade {
                        module_id,
                        old_version,
                        new_version: new_agreed_version,
                    })
                    .encode(),
                );
            }
        }

        Ok(())
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
                    peer_set: updated_peer_set.clone(),
                })
                .encode(),
            );

            // Check for module version upgrades after peer set change
            let mut modules_configs_tbl = dbtx.open_table(&tables::modules_configs::TABLE)?;
            let mut versions_votes_tbl = dbtx.open_table(&tables::modules_versions_votes::TABLE)?;
            self.check_module_version_upgrades(
                dbtx,
                &mut modules_configs_tbl,
                &mut versions_votes_tbl,
                &updated_peer_set,
                &mut effects,
            )?;
        }

        Ok(effects)
    }

    fn process_citem_vote_module_version(
        &self,
        dbtx: &ModuleWriteTransactionCtx,
        voter_pubkey: PeerPubkey,
        peer_set: &PeerSet,
        module_id: ModuleId,
        minor_consensus_version: ConsensusVersionMinor,
    ) -> DbTxResult<Vec<CItemEffect>, Whatever> {
        // Verify the voter is actually in the current peer set
        if !peer_set.contains(&voter_pubkey) {
            None.whatever_context("Voter is not in the current peer set")
                .context(TxSnafu)?;
        }

        // Open the votes table for reading and writing
        let mut versions_votes_tbl = dbtx.open_table(&tables::modules_versions_votes::TABLE)?;

        // Get the current module configuration to create the version
        let mut modules_configs_tbl = dbtx.open_table(&tables::modules_configs::TABLE)?;
        let current_config = modules_configs_tbl
            .get(&module_id)?
            .whatever_context("Module not found in configurations")
            .context(TxSnafu)?
            .value();

        // Create new version combining current major with voted minor
        let new_version =
            ConsensusVersion::new(current_config.version.major(), minor_consensus_version);

        // Record the vote
        versions_votes_tbl.insert(&(voter_pubkey, module_id), &new_version)?;

        // If this vote is from ourselves, clear the pending vote
        if Some(voter_pubkey) == self.peer_pubkey {
            dbtx.open_table(&tables::pending_modules_versions_votes::TABLE)?
                .remove(&module_id)?;
        }

        // Check for module version upgrades across all modules
        let mut effects = vec![];
        self.check_module_version_upgrades(
            dbtx,
            &mut modules_configs_tbl,
            &mut versions_votes_tbl,
            peer_set,
            &mut effects,
        )?;

        Ok(effects)
    }

    pub fn init_db_tx(dbtx: &ModuleWriteTransactionCtx) -> DbResult<()> {
        dbtx.open_table(&tables::modules_configs::TABLE)?;
        dbtx.open_table(&tables::peers::TABLE)?;
        dbtx.open_table(&tables::add_peer_votes::TABLE)?;
        dbtx.open_table(&tables::remove_peer_votes::TABLE)?;
        dbtx.open_table(&tables::pending_add_peer_vote::TABLE)?;
        dbtx.open_table(&tables::pending_remove_peer_vote::TABLE)?;
        dbtx.open_table(&tables::modules_versions_votes::TABLE)?;
        dbtx.open_table(&tables::pending_modules_versions_votes::TABLE)?;
        Ok(())
    }

    pub async fn record_module_init_versions(
        &self,
        modules_supported_versions: &BTreeMap<ModuleKind, ModuleSupportedConsensusVersions>,
    ) {
        let module_configs = self.get_modules_configs().await;

        self.db
            .write_with_expect(|dbtx| {
                let mut pending_votes_tbl =
                    dbtx.open_table(&tables::pending_modules_versions_votes::TABLE)?;

                for (module_id, module_config) in module_configs {
                    let supported_versions = modules_supported_versions
                        .get(&module_config.kind)
                        .unwrap_or_else(|| {
                            panic!(
                                "Missing module supported versions for kind: {}",
                                module_config.kind
                            )
                        });

                    let current_major = module_config.version.major();

                    let max_minor = supported_versions.get(&current_major).unwrap_or_else(|| {
                        panic!(
                            "No supported minor version for major version {} in module {}",
                            current_major, module_id
                        )
                    });

                    pending_votes_tbl.insert(&module_id, max_minor)?;
                }

                Ok(())
            })
            .await;
    }
}

#[async_trait]
impl IModule for AppConsensusModule {
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
            AppConsensusCitem::VoteModuleVersion {
                module_id,
                minor_consensus_version,
            } => self.process_citem_vote_module_version(
                dbtx,
                peer_pubkey,
                peer_set,
                module_id,
                minor_consensus_version,
            ),
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
