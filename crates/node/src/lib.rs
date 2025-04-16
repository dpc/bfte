//! BFTE Node
//!
//! A BFTE node follows and possibly participates in extending
//! some consensus maintained as a blockchain, persisting neccessary
//! data in a database.
//!
//! This crate drives [`bfte-consensus`] for actual consensus logic,
//! taking care of communication with other peers based on the consensus
//! state.
//!
//! See [`run_consensus`] for the core consensus round loop logic.
mod connection_pool;
pub mod derive_secret_ext;
mod finality_vote_query_task;
mod handle;
mod invite;
mod join;
mod peer_address;
pub(crate) mod rpc;
mod rpc_server;
mod run_consensus;
mod ui_api;

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;

use backon::FibonacciBuilder;
use bfte_consensus::consensus::Consensus;
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::peer::{PeerPubkey, PeerSeckey};
use bfte_db::Database;
use bfte_db::error::DbError;
use bfte_derive_secret::{DeriveableSecret, LevelError};
use bfte_node_ui::RunUiFn;
use bfte_util_error::fmt::FmtCompact as _;
use bfte_util_error::{Whatever, WhateverResult};
use bfte_util_fmt_opt::AsFmtOption as _;
use connection_pool::{ALPN_BFTE_V0, ConnectionPool};
use derive_secret_ext::DeriveSecretExt as _;
use handle::NodeHandle;
use iroh::protocol::Router;
use n0_future::task::AbortOnDropHandle;
use snafu::{ResultExt as _, Snafu};
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

const LOG_TARGET: &str = "bfte::node";
const RPC_BACKOFF: FibonacciBuilder = FibonacciBuilder::new()
    .with_jitter()
    .without_max_times()
    .with_max_delay(Duration::from_secs(60));

pub struct Node {
    #[allow(dead_code)]
    /// Weak handle to self
    handle: NodeHandle,
    /// Raw version of [`handle`]
    handle_raw: Weak<Node>,

    /// database everything is in
    db: Arc<Database>,

    /// Optional root secret we're running the peer with
    root_secret: Option<DeriveableSecret>,
    /// Optional peer pubkey derived from [`root_secret`]
    peer_pubkey: Option<PeerPubkey>,

    /// Iroh endpoint
    iroh_endpoint: iroh::Endpoint,
    /// Iroh router handling rpcs
    #[allow(dead_code /* only for drop */)]
    iroh_router: iroh::protocol::Router,

    /// Consensus database and logic
    consensus: Arc<Consensus>,

    /// Connection pool
    connection_pool: ConnectionPool,

    /// Tasks querying peers for finality votes
    finality_tasks: Mutex<BTreeMap<PeerPubkey, AbortOnDropHandle<()>>>,
    #[allow(dead_code /* only for drop */)]
    ui_task: Option<AbortOnDropHandle<WhateverResult<Infallible>>>,

    /// Set each time a peer address requires refreshing
    peer_addr_needed: Arc<Notify>,
}

#[derive(Debug, Snafu)]
pub enum NodeInitError {
    Db {
        source: DbError,
    },
    #[snafu(transparent)]
    Secret {
        source: LevelError,
    },
    IrohEndpoint {
        source: anyhow::Error,
    },
    IrohRouter {
        source: Whatever,
    },
    #[snafu(transparent)]
    ConsensusOpen {
        source: bfte_consensus::consensus::OpenError,
    },
    #[snafu(transparent)]
    ConsensusInit {
        source: bfte_consensus::consensus::InitError,
    },
}

pub type NodeInitResult<T> = Result<T, NodeInitError>;

#[bon::bon]
impl Node {
    #[builder]
    pub async fn new(
        root_secret: Option<DeriveableSecret>,
        db_path: Option<PathBuf>,
        ui: Option<RunUiFn>,
    ) -> NodeInitResult<Arc<Self>> {
        let db = if let Some(db_path) = db_path {
            info!(target: LOG_TARGET, path = %db_path.display(), "Opening redb database");
            Database::open(db_path).await.context(DbSnafu)?
        } else {
            warn!(target: LOG_TARGET, "Using ephemeral in-memory database");
            Database::new_in_memory().await.context(DbSnafu)?
        };

        let peer_pubkey = if let Some(root_secret) = root_secret {
            Some(root_secret.get_peer_seckey()?.pubkey())
        } else {
            None
        };
        let iroh_endpoint = Self::make_iroh_endpoint(if let Some(root_secret) = root_secret {
            Some(root_secret.get_iroh_secret()?)
        } else {
            None
        })
        .await
        .context(IrohEndpointSnafu)?;

        let db = Arc::new(db);
        let consensus = Arc::new(Consensus::open(db.clone(), peer_pubkey).await?);

        let slf = Arc::new_cyclic(|weak| {
            let handle = NodeHandle::from(weak.clone());
            let iroh_router = Self::make_iroh_router(handle.clone(), iroh_endpoint.clone());

            let ui_task = ui.map(|ui| Self::spawn_ui_task(handle.clone(), ui));

            Node {
                handle: handle.clone(),
                handle_raw: weak.clone(),
                iroh_router,
                peer_pubkey,
                db: db.clone(),
                connection_pool: ConnectionPool::new(handle, db, iroh_endpoint.clone()),
                root_secret,
                iroh_endpoint,
                consensus,
                finality_tasks: Mutex::new(BTreeMap::default()),
                ui_task,
                peer_addr_needed: Arc::new(Notify::new()),
            }
        });

        if let Err(err) = slf.insert_own_address_update().await {
            debug!(
                target: LOG_TARGET,
                err = %err.fmt_compact(),
                "Failed to update own address"
            );
        }
        Ok(slf)
    }
}

impl Node {
    pub async fn create(
        db_path: PathBuf,
        root_secret: DeriveableSecret,
        extra_peers: Vec<PeerPubkey>,
    ) -> NodeInitResult<()> {
        let pubkey = root_secret.get_peer_seckey()?.pubkey();

        let params = ConsensusParams {
            applied_round: 0.into(),
            prev_mid_block: None,
            version: Consensus::VERSION,
            peers: [vec![pubkey], extra_peers].concat(),
        };

        let db = Database::open(db_path).await.context(DbSnafu)?;

        let _consensus = Consensus::init(&params, db.into(), Some(pubkey), None).await?;

        Ok(())
    }

    fn clone_strong(&self) -> Arc<Self> {
        self.handle_raw.upgrade().expect("Can't fail")
    }

    async fn make_iroh_endpoint(
        iroh_secret: Option<iroh::SecretKey>,
    ) -> anyhow::Result<iroh::Endpoint> {
        let builder = iroh::Endpoint::builder().discovery_n0();

        let endpoint = if let Some(iroh_secret) = iroh_secret {
            builder.secret_key(iroh_secret)
        } else {
            builder
        };

        let iroh_endpoint = endpoint.bind().await?;
        let (iroh_addr, iroh_addr_ipv6) = iroh_endpoint.bound_sockets();
        info!(
            target: LOG_TARGET,
            endpoint = %iroh_endpoint.node_id(),
            bound_ipv4 = %iroh_addr,
            bound_ipv6 = %iroh_addr_ipv6.fmt_option(),
            "Iroh endpoint initialized"
        );

        Ok(iroh_endpoint)
    }

    fn make_iroh_router(
        handle: NodeHandle,
        iroh_endpoint: iroh::Endpoint,
    ) -> iroh::protocol::Router {
        let rpc = rpc_server::RpcServer::new(handle.clone()).into_iroh_protocol_handler();

        Router::builder(iroh_endpoint)
            .accept(ALPN_BFTE_V0, rpc)
            .spawn()
    }

    fn get_peer_secret_expect(&self) -> PeerSeckey {
        self.root_secret
            .expect("Must contain root secret to participate")
            .get_peer_seckey()
            .expect("Level verified by now")
    }

    pub async fn run(self: Arc<Self>) -> WhateverResult<()> {
        info!(
            target: LOG_TARGET,
            peer_pubkey = %self.peer_pubkey.fmt_option(),
            "Starting node..."
        );
        let invite = self.generate_invite_code().await;
        info!(target: LOG_TARGET, %invite, "Invite code");

        let mut tasks = JoinSet::new();

        tasks.spawn(self.clone().run_consensus());
        tasks.spawn(self.clone().run_push_gossip());
        tasks.spawn(self.clone().run_pull_gossip());

        tasks
            .join_next()
            .await
            .expect("At least one task is there")
            .whatever_context("Task failed")
    }
}
