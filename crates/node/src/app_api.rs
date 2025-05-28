use std::convert::Infallible;
use std::sync::Arc;

use async_trait::async_trait;
use bfte_consensus_core::block::{BlockHeader, BlockPayloadRaw, BlockRound};
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_node_app_core::{INodeAppApi, RunNodeAppFn};
use bfte_util_error::WhateverResult;
use n0_future::task::AbortOnDropHandle;
use snafu::ResultExt as _;

use crate::Node;
use crate::handle::{NodeHandle, NodeRef};

struct NodeAppApi {
    handle: NodeHandle,
}

impl NodeAppApi {
    fn node_ref(&self) -> WhateverResult<NodeRef<'_>> {
        self.handle
            .node_ref()
            .whatever_context("Node shutting down")
    }
}

#[async_trait]
impl INodeAppApi for NodeAppApi {
    async fn ack_and_wait_next_block(&self, round: BlockRound) -> (BlockHeader, BlockPayloadRaw) {
        todo!()
    }

    async fn schedule_consensus_params(&self, consensus_params: ConsensusParams) {
        todo!()
    }
}

impl Node {
    pub(crate) fn spawn_app_task(
        handle: NodeHandle,
        app: RunNodeAppFn,
    ) -> AbortOnDropHandle<WhateverResult<Infallible>> {
        AbortOnDropHandle::new(tokio::spawn(app(Arc::new(NodeAppApi { handle }))))
    }
}
