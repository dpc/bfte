use std::convert::Infallible;
use std::future;
use std::sync::Arc;

use async_trait::async_trait;
use bfte_consensus::consensus::Consensus;
use bfte_consensus_core::block::{BlockHeader, BlockRound};
use bfte_consensus_core::citem::CItem;
use bfte_consensus_core::citem::transaction::Transaction;
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_db::Database;
use bfte_node_app_core::{INodeAppApi, RunNodeAppFn};
use bfte_node_shared_modules::SharedModules;
use bfte_util_error::WhateverResult;
use bfte_util_error::fmt::FmtCompact as _;
use n0_future::task::AbortOnDropHandle;
use tokio::sync::watch;

use crate::Node;
use crate::handle::{NodeHandle, NodeRef};

struct NodeAppApi {
    handle: NodeHandle,
}

impl NodeAppApi {
    async fn node_ref_wait(&self) -> NodeRef<'_> {
        let Ok(node_ref) = self.handle.node_ref() else {
            future::pending().await
        };

        node_ref
    }
}

#[async_trait]
impl INodeAppApi for NodeAppApi {
    async fn get_consensus(&self) -> Arc<Consensus> {
        self.node_ref_wait().await.consensus_wait().await.clone()
    }
    async fn get_peer_pubkey(&self) -> Option<PeerPubkey> {
        self.node_ref_wait().await.peer_pubkey
    }

    async fn get_consensus_params(&self, round: BlockRound) -> ConsensusParams {
        self.node_ref_wait()
            .await
            .consensus_wait()
            .await
            .get_consensus_params(round)
            .await
    }

    async fn ack_and_wait_next_block<'f>(
        &self,
        mut req_round: BlockRound,
    ) -> (BlockHeader, PeerPubkey, Arc<[CItem]>) {
        let node_ref = self.node_ref_wait().await;

        node_ref.node_app_ack_tx.send_replace(req_round);
        let consensus = node_ref.consensus_wait().await;

        let mut current_round_with_timeout_rx = consensus.current_round_with_timeout_rx();
        let mut finality_consensus_rx = consensus.finality_consensus_rx();

        let block = loop {
            current_round_with_timeout_rx.mark_unchanged();
            // Wait for finality to reach the requested block
            let Ok(cur_finality) = finality_consensus_rx
                .wait_for(|finality| req_round < *finality)
                .await
                .map(|f| *f)
            else {
                future::pending().await
            };

            let Some(block) = consensus.get_next_notarized_block(req_round).await else {
                let _ = current_round_with_timeout_rx.changed().await;
                continue;
            };
            if cur_finality <= block.round {
                // Notarized block can only get replaced with a notarized block
                // of a higher round, so it's OK to
                // to ack cur_finality as node_app_ack, as nothing
                // else can appear in between anymore.
                node_ref.node_app_ack_tx.send_replace(cur_finality);
                req_round = block.round;
                continue;
            }
            break block;
        };

        assert!(block.round < *finality_consensus_rx.borrow());

        let block_payload = consensus
            .get_block_payload(block.payload_hash)
            .await
            .expect("Must have notarized block payload");

        let params = consensus.get_consensus_params(block.round).await;

        let leader_idx = block.round.leader_idx(params.num_peers());

        let peer_pubkey = params
            .peers
            .as_slice()
            .get(leader_idx.as_usize())
            .expect("Must exist");

        let block_payload = block_payload.decode_citems().expect("Can't fail");

        (block, *peer_pubkey, block_payload)
    }
}

impl Node {
    pub(crate) fn spawn_app_task(
        handle: NodeHandle,
        db: Arc<Database>,
        app: RunNodeAppFn,
        shared_modules: SharedModules,
        pending_transactions_tx: watch::Sender<Vec<Transaction>>,
    ) -> AbortOnDropHandle<WhateverResult<Infallible>> {
        AbortOnDropHandle::new(tokio::spawn(async move {
            app(
                db,
                Arc::new(NodeAppApi { handle }),
                shared_modules,
                pending_transactions_tx,
            )
            .await
            .inspect_err(|err| {
                panic!("Node app level processing failed: {}", err.fmt_compact());
            })
        }))
    }
}
