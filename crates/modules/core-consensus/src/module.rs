use std::collections::BTreeMap;

use async_trait::async_trait;
use bfte_consensus_core::bincode::CONSENSUS_BINCODE_CONFIG;
use bfte_consensus_core::block::BlockRound;
use bfte_consensus_core::citem::{CItemRaw, InputRaw, OutputRaw};
use bfte_consensus_core::module::ModuleId;
use bfte_consensus_core::num_peers::ToNumPeers;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_consensus_core::peer_set::PeerSet;
use bfte_consensus_core::ver::ConsensusVersion;
use bfte_db::error::TxSnafu;
use bfte_module::effect::{CItemEffect, EffectId, ModuleCItemEffect};
use bfte_module::module::IModule;
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::db::{DbResult, DbTxResult, ModuleDatabase, ModuleWriteTransactionCtx};
use bfte_util_error::{Whatever, WhateverResult};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use snafu::{OptionExt as _, ResultExt as _, whatever};
use tokio::sync::watch;

use crate::tables;

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub enum CoreConsensusCitem {
    VoteAddPeer(PeerPubkey),
}

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub enum CoreConsensusCItemEffect {
    AddPeer(PeerPubkey),
}

impl CoreConsensusCitem {
    pub fn to_citem_raw(&self) -> CItemRaw {
        let serialized = bincode::encode_to_vec(self, CONSENSUS_BINCODE_CONFIG)
            .expect("encoding should not fail");
        CItemRaw(serialized.into())
    }

    pub fn from_citem_raw(citem_raw: &CItemRaw) -> WhateverResult<Self> {
        match bincode::decode_from_slice(citem_raw, CONSENSUS_BINCODE_CONFIG) {
            Ok((citem, _)) => Ok(citem),
            Err(e) => whatever!("Failed to decode CoreConsensusCitem: {e}"),
        }
    }
}

pub struct CoreConsensusModule {
    #[allow(dead_code)]
    pub(crate) version: ConsensusVersion,
    pub(crate) db: ModuleDatabase,
    pub(crate) peer_pubkey: Option<PeerPubkey>,
    pub(crate) propose_citems_rx: watch::Receiver<Vec<CItemRaw>>,
    pub(crate) propose_citems_tx: watch::Sender<Vec<CItemRaw>>,
}

impl CoreConsensusModule {
    pub async fn get_modules_configs(&self) -> BTreeMap<ModuleId, ModuleConfig> {
        self.db
            .read_with_expect(|dbtx| {
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

    async fn refresh_consensus_proposals(&self) {
        let mut proposals = Vec::new();

        let Some(peer_pubkey) = self.peer_pubkey else {
            return;
        };

        let pending_vote = self
            .db
            .read_with_expect(|dbtx| {
                let tbl = dbtx.open_table(&tables::pending_add_peer_vote::TABLE)?;
                Ok(tbl.get(&())?.map(|v| v.value()))
            })
            .await;

        if let Some(pending_peer) = pending_vote {
            let current_vote = self
                .db
                .read_with_expect(|dbtx| {
                    let tbl = dbtx.open_table(&tables::add_peer_votes::TABLE)?;
                    Ok(tbl.get(&peer_pubkey)?.map(|v| v.value()))
                })
                .await;

            if current_vote != Some(pending_peer) {
                let citem = CoreConsensusCitem::VoteAddPeer(pending_peer);
                proposals.push(citem.to_citem_raw());
            }
        }

        self.propose_citems_tx.send_replace(proposals);
    }

    fn process_vote_add_peer(
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
            // This peer should be added to the consensus
            effects.push(CItemEffect {
                effect_id: EffectId::new(0), // Use effect ID 0 for AddPeer
                raw: bincode::encode_to_vec(
                    CoreConsensusCItemEffect::AddPeer(peer_to_add),
                    CONSENSUS_BINCODE_CONFIG,
                )
                .expect("encoding should not fail")
                .into(),
            });
        }

        Ok(effects)
    }

    pub fn init_db_tx(dbtx: &ModuleWriteTransactionCtx) -> DbResult<()> {
        dbtx.open_table(&tables::modules_configs::TABLE)?;
        dbtx.open_table(&tables::peers::TABLE)?;
        dbtx.open_table(&tables::add_peer_votes::TABLE)?;
        dbtx.open_table(&tables::remove_peer_votes::TABLE)?;
        dbtx.open_table(&tables::pending_add_peer_vote::TABLE)?;
        Ok(())
    }
}

#[async_trait]
impl IModule for CoreConsensusModule {
    fn display_name(&self) -> &'static str {
        "Core Consensus"
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
        let citem = CoreConsensusCitem::from_citem_raw(citem).context(TxSnafu)?;

        match citem {
            CoreConsensusCitem::VoteAddPeer(peer_to_add) => {
                self.process_vote_add_peer(dbtx, peer_pubkey, peer_set, peer_to_add)
            }
        }
    }

    fn process_input(
        &self,
        _dbtx: &ModuleWriteTransactionCtx,
        _input: &InputRaw,
    ) -> DbTxResult<Vec<CItemEffect>, Whatever> {
        todo!()
    }

    fn process_output(
        &self,
        _dbtx: &ModuleWriteTransactionCtx,
        _output: &OutputRaw,
    ) -> DbTxResult<Vec<CItemEffect>, Whatever> {
        todo!()
    }

    fn process_effects(
        &self,
        _dbtx: &ModuleWriteTransactionCtx,
        _effects: &[ModuleCItemEffect],
    ) -> DbTxResult<Vec<CItemEffect>, Whatever> {
        todo!()
    }
}
