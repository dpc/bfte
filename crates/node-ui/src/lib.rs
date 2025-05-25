// SPDX-License-Identifier: MIT

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bfte_consensus_core::block::BlockRound;
use bfte_util_error::WhateverResult;
use tokio::sync::watch;

pub type RunUiFn = Box<
    dyn Fn(NodeUiApi) -> Pin<Box<dyn Future<Output = WhateverResult<Infallible>> + Send>>
        + Send
        + Sync
        + 'static,
>;

pub type NodeUiApi = Arc<dyn INodeUiApi + Send + Sync + 'static>;

/// The interface that UI implementation can use to communicate with the node
#[async_trait]
pub trait INodeUiApi {
    fn get_ui_password_hash(&self) -> WhateverResult<blake3::Hash>;
    async fn change_ui_password(&self, pass: &str) -> WhateverResult<()>;
    fn is_ui_password_temporary(&self) -> WhateverResult<bool>;

    fn is_consensus_initialized(&self) -> WhateverResult<bool>;
    fn get_round_and_timeout_rx(
        &self,
    ) -> WhateverResult<watch::Receiver<(BlockRound, Option<Duration>)>>;
}
