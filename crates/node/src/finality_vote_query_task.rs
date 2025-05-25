use std::sync::Arc;
use std::time::Duration;

use backon::Retryable as _;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_util_error::fmt::FmtCompact as _;
use bfte_util_error::{Whatever, WhateverResult};
use n0_future::task::AbortOnDropHandle;
use snafu::ResultExt as _;
use tracing::{debug, instrument};

use crate::{LOG_TARGET, Node, RPC_BACKOFF, rpc};

impl Node {
    pub(crate) async fn spawn_finality_vote_query_task(self: &Arc<Self>, peer_pubkey: PeerPubkey) {
        let mut write = self.finality_tasks.lock().await;

        if write.contains_key(&peer_pubkey) {
            return;
        }

        write.insert(
            peer_pubkey,
            AbortOnDropHandle::new(tokio::spawn(
                self.clone().run_peer_finality_vote_query_task(peer_pubkey),
            )),
        );
    }

    #[instrument(
        name = "peer_finality_vote_query"
        target = LOG_TARGET,
        skip_all,
        fields(peer_pubkey = %peer_pubkey)
    )]
    async fn run_peer_finality_vote_query_task(self: Arc<Self>, peer_pubkey: PeerPubkey) {
        if self.peer_pubkey == Some(peer_pubkey) {
            // No point querring oneself
            return;
        }
        loop {
            { || async { self.peer_finality_vote_query(peer_pubkey).await } }
                .retry(RPC_BACKOFF)
                .notify(|err: &Whatever, dur: Duration| {
                    debug!(target:
                        LOG_TARGET,
                        dur_millis = %dur.as_millis(),
                        err = %err.fmt_compact(),
                        "Retrying failed finality query rpc"
                    );
                })
                .await
                .expect("Always retry")
        }
    }

    async fn peer_finality_vote_query(&self, peer_pubkey: PeerPubkey) -> WhateverResult<()> {
        let prev_vote = self
            .consensus_expect()
            .get_finality_vote(peer_pubkey)
            .await
            .unwrap_or_default();

        let mut conn = self
            .connection_pool()
            .connect(peer_pubkey)
            .await
            .whatever_context("Failed to connect to peer")?;

        let resp = rpc::wait_finality_vote(&mut conn, peer_pubkey, prev_vote).await?;

        self.consensus_expect()
            .process_finality_vote_update_response(peer_pubkey, resp)
            .await
            .whatever_context("Failed to process finality vote update")?;
        Ok(())
    }
}
