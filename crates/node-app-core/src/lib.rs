// SPDX-License-Identifier: MIT

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use bfte_consensus_core::block::{BlockHeader, BlockRound};
use bfte_consensus_core::citem::CItem;
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_db::Database;
use bfte_node_shared_modules::SharedModules;
use bfte_util_error::WhateverResult;

pub type RunNodeAppFn = Box<
    dyn Fn(
            Arc<Database>,
            NodeAppApi,
            SharedModules,
        ) -> Pin<Box<dyn Future<Output = WhateverResult<Infallible>> + Send>>
        + Send
        + Sync
        + 'static,
>;

pub type NodeAppApi = Arc<dyn INodeAppApi + Send + Sync + 'static>;

/// The API `bfte-node` exposes to `bfte-node-app`
#[async_trait]
pub trait INodeAppApi {
    async fn get_consensus_params(&self, round: BlockRound) -> WhateverResult<ConsensusParams>;

    /// Wait for the first finalized block at `round` or higher
    ///
    /// It also acknowledges that that application logic processed
    /// all blocks before `rounds` (since it asks for next ones)
    async fn ack_and_wait_next_block<'f>(&self, round: BlockRound) -> (BlockHeader, Arc<[CItem]>);

    /// Notify node logic that a [`ConsensusParams`] were scheduled to change
    async fn schedule_consensus_params(&self, consensus_params: ConsensusParams);
}
