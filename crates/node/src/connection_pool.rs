use std::collections::HashMap;
use std::sync::Arc;

use bfte_consensus_core::peer::PeerPubkey;
use bfte_db::Database;
use bfte_util_error::Whatever;
use iroh::endpoint::Connection;
use snafu::{OptionExt as _, ResultExt as _, Snafu};
use tracing::trace;

use crate::Node;
use crate::handle::{NodeHandle, NodeRefError};

pub const ALPN_BFTE_V0: &[u8] = b"bfte-p2p-v0";

const LOG_TARGET: &str = "bfte::node::conn-pool";

#[derive(Debug, Snafu)]
pub(crate) enum ConnectError {
    IrohConnect {
        source: anyhow::Error,
    },
    #[snafu(transparent)]
    NodeRef {
        source: NodeRefError,
    },
    AddressResolve {
        source: Whatever,
    },
    #[snafu(display("Unknown peer: {peer_pubkey}"))]
    UnknownPeer {
        peer_pubkey: PeerPubkey,
    },
}

pub(crate) type ConnectResult<T> = Result<T, ConnectError>;
/// Connection pool keeps around existing connections so they can be cheaply
/// reused
#[derive(Clone)]
pub(crate) struct ConnectionPool {
    node_handle: NodeHandle,
    db: Arc<Database>,
    endpoint: iroh::Endpoint,
    iroh_connections: Arc<tokio::sync::Mutex<HashMap<iroh::NodeId, Connection>>>,
}

impl ConnectionPool {
    pub(crate) fn new(
        node_handle: NodeHandle,
        db: Arc<Database>,
        endpoint: iroh::Endpoint,
    ) -> Self {
        Self {
            node_handle,
            iroh_connections: tokio::sync::Mutex::new(Default::default()).into(),
            endpoint,
            db,
        }
    }

    pub(crate) async fn connect(&self, peer_pubkey: PeerPubkey) -> ConnectResult<Connection> {
        trace!(target: LOG_TARGET, %peer_pubkey, "Getting connection");
        let node_id: iroh::NodeId = Node::get_peer_iroh_addr(&self.db, peer_pubkey)
            .await
            .context(AddressResolveSnafu)?
            .context(UnknownPeerSnafu { peer_pubkey })?;

        if let Some(conn) = self.iroh_connections.lock().await.get(&node_id) {
            if conn.close_reason().is_none() {
                trace!(target: LOG_TARGET, %peer_pubkey, "Using existing connection");
                return Ok(conn.clone());
            }
        }

        let node_ref = &self.node_handle.node_ref()?;
        trace!(target: LOG_TARGET, %peer_pubkey, "Trying to make a new connection");
        let conn = match self
            .endpoint
            .connect(node_id, ALPN_BFTE_V0)
            .await
            .context(IrohConnectSnafu)
        {
            Ok(conn) => conn,
            Err(err) => {
                let _ = node_ref.mark_peer_addr_as_needed(peer_pubkey).await;
                return Err(err);
            }
        };

        trace!(target: LOG_TARGET, %peer_pubkey, "Adding new connection to the pool");
        self.iroh_connections
            .lock()
            .await
            .insert(node_id, conn.clone());

        trace!(target: LOG_TARGET, %peer_pubkey, "Returning the connection");

        Ok(conn)
    }
}
