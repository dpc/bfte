use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bfte_consensus_core::block::BlockRound;
use bfte_node_ui::{INodeUiApi, RunUiFn};
use bfte_util_error::WhateverResult;
use n0_future::task::AbortOnDropHandle;
use snafu::ResultExt as _;
use tokio::sync::watch;

use crate::Node;
use crate::handle::NodeHandle;

struct NodeUiApi {
    handle: NodeHandle,
}

#[async_trait]
impl INodeUiApi for NodeUiApi {
    fn get_round_and_timeout_rx(
        &self,
    ) -> WhateverResult<watch::Receiver<(BlockRound, Option<Duration>)>> {
        Ok(self
            .handle
            .node_ref()
            .whatever_context("Node shutting down")?
            .consensus
            .current_round_with_timeout_start_rx())
    }
}

impl Node {
    pub(crate) fn spawn_ui_task(
        handle: NodeHandle,
        ui: RunUiFn,
    ) -> AbortOnDropHandle<WhateverResult<Infallible>> {
        AbortOnDropHandle::new(tokio::spawn(ui(Arc::new(NodeUiApi { handle }))))
    }
}
