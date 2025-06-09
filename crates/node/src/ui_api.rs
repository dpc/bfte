use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use async_trait::async_trait;
use bfte_consensus_core::block::BlockRound;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_invite::Invite;
use bfte_node_shared_modules::WeakSharedModules;
use bfte_node_ui::{INodeUiApi, RunUiFn};
use bfte_util_error::WhateverResult;
use n0_future::task::AbortOnDropHandle;
use snafu::{OptionExt as _, ResultExt as _};
use tokio::sync::watch;

use crate::Node;
use crate::handle::{NodeHandle, NodeRef};

struct NodeUiApi {
    handle: NodeHandle,
}

impl NodeUiApi {
    fn node_ref(&self) -> WhateverResult<NodeRef<'_>> {
        self.handle
            .node_ref()
            .whatever_context("Node shutting down")
    }
}

#[async_trait]
impl INodeUiApi for NodeUiApi {
    fn get_ui_password_hash(&self) -> WhateverResult<blake3::Hash> {
        Ok(*self
            .node_ref()?
            .ui_pass_hash()
            .lock()
            .expect("Locking failed"))
    }

    async fn change_ui_password(&self, pass: &str) -> WhateverResult<()> {
        self.node_ref()?.change_ui_pass(pass).await;
        Ok(())
    }

    fn is_ui_password_temporary(&self) -> WhateverResult<bool> {
        Ok(self
            .node_ref()?
            .ui_pass_is_temporary()
            .load(Ordering::Relaxed))
    }

    fn is_consensus_initialized(&self) -> WhateverResult<bool> {
        Ok(self.node_ref()?.consensus().is_some())
    }

    async fn consensus_init(&self, extra_peers: Vec<PeerPubkey>) -> WhateverResult<()> {
        Ok(self
            .node_ref()?
            .consensus_init(extra_peers)
            .await
            .whatever_context("Failed to join consensus")?)
    }
    async fn consensus_join(&self, invite: &Invite) -> WhateverResult<()> {
        Ok(self
            .node_ref()?
            .consensus_join(invite)
            .await
            .whatever_context("Failed to join consensus")?)
    }

    fn get_round_and_timeout_rx(&self) -> WhateverResult<watch::Receiver<(BlockRound, bool)>> {
        Ok(self
            .node_ref()?
            .consensus()
            .as_ref()
            .whatever_context("Consensus not initialized")?
            .current_round_with_timeout_rx())
    }

    fn get_finality_consensus_rx(&self) -> WhateverResult<watch::Receiver<BlockRound>> {
        Ok(self
            .node_ref()?
            .consensus()
            .as_ref()
            .whatever_context("Consensus not initialized")?
            .finality_consensus_rx())
    }

    fn get_finality_self_vote_rx(&self) -> WhateverResult<watch::Receiver<BlockRound>> {
        Ok(self
            .node_ref()?
            .consensus()
            .as_ref()
            .whatever_context("Consensus not initialized")?
            .finality_self_vote_rx())
    }

    fn get_node_app_ack_rx(&self) -> WhateverResult<watch::Receiver<BlockRound>> {
        Ok(self.node_ref()?.node_app_ack_rx.clone())
    }

    fn get_peer_pubkey(&self) -> WhateverResult<Option<PeerPubkey>> {
        Ok(self.node_ref()?.peer_pubkey)
    }

    fn is_database_ephemeral(&self) -> WhateverResult<bool> {
        Ok(self.node_ref()?.db().is_ephemeral())
    }

    async fn generate_invite_code(&self) -> WhateverResult<Invite> {
        self.node_ref()?.generate_invite_code().await
    }
}

impl Node {
    pub(crate) fn spawn_ui_task(
        handle: NodeHandle,
        ui: RunUiFn,
        shared_modules: WeakSharedModules,
    ) -> AbortOnDropHandle<WhateverResult<Infallible>> {
        AbortOnDropHandle::new(tokio::spawn(ui(
            Arc::new(NodeUiApi { handle }),
            shared_modules,
        )))
    }
}
