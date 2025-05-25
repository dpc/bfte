use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use async_trait::async_trait;
use bfte_consensus_core::block::BlockRound;
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
    fn node_ref(&self) -> std::result::Result<NodeRef<'_>, bfte_util_error::Whatever> {
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
    fn get_round_and_timeout_rx(
        &self,
    ) -> WhateverResult<watch::Receiver<(BlockRound, Option<Duration>)>> {
        Ok(self
            .node_ref()?
            .consensus()
            .as_ref()
            .whatever_context("Consensus not initialized")?
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
