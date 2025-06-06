use std::collections::BTreeMap;

use async_trait::async_trait;
use bfte_consensus_core::bincode::CONSENSUS_BINCODE_CONFIG;
use bfte_consensus_core::block::BlockRound;
use bfte_consensus_core::citem::{CItemRaw, InputRaw, OutputRaw};
use bfte_consensus_core::module::ModuleId;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_consensus_core::peer_set::PeerSet;
use bfte_consensus_core::ver::ConsensusVersion;
use bfte_module::effect::{CItemEffect, ModuleCItemEffect};
use bfte_module::module::IModule;
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::db::{ModuleDatabase, ModuleReadTransaction, ModuleWriteTransactionCtx};
use bfte_util_error::WhateverResult;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use snafu::whatever;
use tokio::sync::watch;

use crate::tables;

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub enum CoreConsensusCitem {
    VoteAddPeer(PeerPubkey),
}

impl CoreConsensusCitem {
    pub fn to_citem_raw(&self) -> CItemRaw {
        let serialized = bincode::encode_to_vec(self, CONSENSUS_BINCODE_CONFIG)
            .expect("encoding should not fail");
        CItemRaw(serialized.into())
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
        _dbtx: &ModuleReadTransaction,
        _round: BlockRound,
        _peer_pubkey: PeerPubkey,
        _citem: &CItemRaw,
    ) -> WhateverResult<Vec<CItemEffect>> {
        todo!()
    }

    fn process_input(
        &self,
        _dbtx: &ModuleReadTransaction,
        _input: &InputRaw,
    ) -> WhateverResult<Vec<CItemEffect>> {
        todo!()
    }
    fn process_output(
        &self,
        _dbtx: &ModuleReadTransaction,
        _output: &OutputRaw,
    ) -> WhateverResult<Vec<CItemEffect>> {
        todo!()
    }

    fn process_effects(
        &self,
        _dbtx: &ModuleWriteTransactionCtx,
        _effects: &[ModuleCItemEffect],
    ) -> WhateverResult<()> {
        todo!()
    }
}
