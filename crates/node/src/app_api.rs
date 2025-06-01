use std::convert::Infallible;
use std::future;
use std::sync::Arc;

use async_trait::async_trait;
use bfte_consensus_core::block::{BlockHeader, BlockRound};
use bfte_consensus_core::citem::CItem;
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_db::Database;
use bfte_node_app_core::{INodeAppApi, RunNodeAppFn};
use bfte_node_shared_modules::SharedModules;
use bfte_util_error::WhateverResult;
use bfte_util_error::fmt::FmtCompact as _;
use n0_future::task::AbortOnDropHandle;
use snafu::ResultExt as _;

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
    async fn get_consensus_params(&self, round: BlockRound) -> ConsensusParams {
        self.node_ref_wait()
            .await
            .consensus_wait()
            .await
            .get_consensus_params(round)
            .await
    }

    async fn ack_and_wait_next_block<'f>(&self, round: BlockRound) -> (BlockHeader, Arc<[CItem]>) {
        let node_ref = self.node_ref_wait().await;

        node_ref.node_app_ack_tx.send_replace(round);

        todo!()
    }

    async fn schedule_consensus_params(&self, _consensus_params: ConsensusParams) {
        todo!()
    }
}

impl Node {
    pub(crate) fn spawn_app_task(
        handle: NodeHandle,
        db: Arc<Database>,
        app: RunNodeAppFn,
        shared_modules: SharedModules,
    ) -> AbortOnDropHandle<WhateverResult<Infallible>> {
        AbortOnDropHandle::new(tokio::spawn(async move {
            app(db, Arc::new(NodeAppApi { handle }), shared_modules)
                .await
                .inspect_err(|err| {
                    panic!("Node app level processing failed: {}", err.fmt_compact());
                })
        }))
    }
}
