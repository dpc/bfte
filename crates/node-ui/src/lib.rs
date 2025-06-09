// SPDX-License-Identifier: MIT

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use bfte_consensus_core::block::BlockRound;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_invite::Invite;
use bfte_node_shared_modules::WeakSharedModules;
use bfte_util_error::WhateverResult;
use tokio::sync::watch;

pub type RunUiFn = Box<
    dyn Fn(
            NodeUiApi,
            WeakSharedModules,
        ) -> Pin<Box<dyn Future<Output = WhateverResult<Infallible>> + Send>>
        + Send
        + Sync
        + 'static,
>;

pub type NodeUiApi = Arc<dyn INodeUiApi + Send + Sync + 'static>;

/// The API `bfte-node` exposes to `bfte-node-ui`
///
/// UI implementation can use this API to get stuff from the node.
#[async_trait]
pub trait INodeUiApi {
    fn get_ui_password_hash(&self) -> WhateverResult<blake3::Hash>;
    async fn change_ui_password(&self, pass: &str) -> WhateverResult<()>;
    fn is_ui_password_temporary(&self) -> WhateverResult<bool>;

    fn is_consensus_initialized(&self) -> WhateverResult<bool>;
    async fn consensus_init(&self, extra_peers: Vec<PeerPubkey>) -> WhateverResult<()>;
    async fn consensus_join(&self, invite: &Invite) -> WhateverResult<()>;

    fn get_round_and_timeout_rx(&self) -> WhateverResult<watch::Receiver<(BlockRound, bool)>>;
    fn get_finality_consensus_rx(&self) -> WhateverResult<watch::Receiver<BlockRound>>;
    fn get_finality_self_vote_rx(&self) -> WhateverResult<watch::Receiver<BlockRound>>;
    fn get_node_app_ack_rx(&self) -> WhateverResult<watch::Receiver<BlockRound>>;
    async fn generate_invite_code(&self) -> WhateverResult<Invite>;
}
