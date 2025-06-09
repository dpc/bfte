mod getters;
mod init;

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::future;
use std::option::Option;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock, Weak};

use bfte_consensus::consensus::{Consensus, OpenError};
use bfte_consensus_core::block::BlockRound;
use bfte_consensus_core::citem::transaction::Transaction;
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::peer::{PeerPubkey, PeerSeckey};
use bfte_consensus_core::timestamp::Timestamp;
use bfte_consensus_core::ver::ConsensusVersion;
use bfte_db::Database;
use bfte_db::error::DbError;
use bfte_derive_secret::{DeriveableSecret, LevelError};
use bfte_invite::Invite;
use bfte_node_app_core::RunNodeAppFn;
use bfte_node_shared_modules::{SharedModules, WeakSharedModules};
use bfte_node_ui::RunUiFn;
use bfte_util_error::fmt::FmtCompact as _;
use bfte_util_error::{Whatever, WhateverResult};
use bfte_util_fmt_opt::AsFmtOption as _;
use connection_pool::{ALPN_BFTE_V0, ConnectionPool};
use derive_secret_ext::DeriveSecretExt as _;
use handle::NodeHandle;
use iroh::protocol::Router;
use n0_future::task::AbortOnDropHandle;
use rand::Rng as _;
use rand::distributions::Alphanumeric;
use snafu::{ResultExt as _, Snafu};
use tokio::sync::{Mutex, Notify, watch};
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use crate::join::NodeJoinResult;
use crate::{LOG_TARGET, connection_pool, derive_secret_ext, handle, rpc_server};

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
    pub(crate) peer_pubkey: Option<PeerPubkey>,

    /// Iroh endpoint
    iroh_endpoint: iroh::Endpoint,
    /// Iroh router handling rpcs
    #[allow(dead_code /* only for drop */)]
    iroh_router: iroh::protocol::Router,

    /// Consensus database and logic
    consensus: OnceLock<Arc<Consensus>>,
    consensus_initialized_rx: watch::Receiver<bool>,
    consensus_initialized_tx: watch::Sender<bool>,

    /// Connection pool
    connection_pool: ConnectionPool,

    /// Block round acknowledged by the node app processing
    pub(crate) node_app_ack_rx: watch::Receiver<BlockRound>,
    pub(crate) node_app_ack_tx: watch::Sender<BlockRound>,

    pub(crate) pending_transactions_rx: watch::Receiver<Vec<Transaction>>,

    /// Tasks querying peers for finality votes
    pub(crate) finality_tasks: Mutex<BTreeMap<PeerPubkey, AbortOnDropHandle<()>>>,
    #[allow(dead_code /* only for drop */)]
    ui_task: Option<AbortOnDropHandle<WhateverResult<Infallible>>>,
    #[allow(dead_code /* only for drop */)]
    app_task: Option<AbortOnDropHandle<WhateverResult<Infallible>>>,
    #[allow(dead_code /* TODO */)]
    pub(crate) weak_shared_modules: WeakSharedModules,

    ui_pass_hash: std::sync::Mutex<blake3::Hash>,
    ui_pass_is_temporary: AtomicBool,

    /// Set each time a peer address requires refreshing
    peer_addr_needed: Arc<Notify>,

    /// If bootstrapping new consensus, use this version for the module
    ///
    /// It is so specific, because Node can initialize consensus on the UI
    /// demand, but other than this piece of data, it doesn't need to know
    /// anything about modules.
    app_consensus_module_init_consensus_version: ConsensusVersion,
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
    NoSecret,
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
    pub async fn open_db(db_path: Option<PathBuf>) -> NodeInitResult<Database> {
        let db = if let Some(db_path) = db_path {
            info!(target: LOG_TARGET, path = %db_path.display(), "Opening redb database…");
            Database::open(db_path).await.context(DbSnafu)?
        } else {
            warn!(target: LOG_TARGET, "Using ephemeral in-memory database!");
            Database::new_in_memory().await.context(DbSnafu)?
        };

        db.write_with_expect(Self::init_tables_tx).await;

        Ok(db)
    }

    #[builder]
    pub async fn new(
        root_secret: Option<DeriveableSecret>,
        db: Arc<Database>,
        app_consensus_module_init_consensus_version: ConsensusVersion,
        ui: Option<RunUiFn>,
        app: Option<RunNodeAppFn>,
        force_ui_password: Option<String>,
    ) -> NodeInitResult<Arc<Self>> {
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

        if let Some(force_ui_pass) = force_ui_password {
            Self::change_ui_pass_db_static(&db, &force_ui_pass).await;
        }

        #[allow(clippy::manual_ok_err)] // might change signature soon
        let consensus = match Consensus::open(db.clone(), peer_pubkey).await {
            Ok(c) => Some(c),
            Err(OpenError::NotInitialized) => None,
        };

        let (ui_pass_hash, ui_pass_is_temporary) = Self::load_ui_pass_hash(&db)
            .await
            .map(|h| (h, false))
            .unwrap_or_else(|| {
                let pass = gen_random_pass();
                warn!(target: LOG_TARGET, %pass, "Temporary UI password");

                (blake3::hash(pass.as_bytes()), true)
            });

        let slf = Arc::new_cyclic(|weak| {
            let shared_modules = SharedModules::default();
            let weak_shared_modules = shared_modules.downgrade();

            let handle = NodeHandle::from(weak.clone());
            let iroh_router = Self::make_iroh_router(handle.clone(), iroh_endpoint.clone());

            let ui_task =
                ui.map(|ui| Self::spawn_ui_task(handle.clone(), ui, weak_shared_modules.clone()));

            let (consensus_initialized_tx, consensus_initialized_rx) =
                watch::channel(consensus.is_some());
            let (pending_transactions_tx, pending_transactions_rx) = watch::channel(vec![]);
            let (node_app_ack_tx, node_app_ack_rx) = watch::channel(BlockRound::ZERO);

            let app_task = app.map(|app| {
                Self::spawn_app_task(
                    handle.clone(),
                    db.clone(),
                    app,
                    shared_modules,
                    pending_transactions_tx,
                )
            });
            let node = Node {
                handle: handle.clone(),
                handle_raw: weak.clone(),
                iroh_router,
                peer_pubkey,
                db: db.clone(),
                connection_pool: ConnectionPool::new(handle, db, iroh_endpoint.clone()),
                root_secret,
                iroh_endpoint,
                consensus_initialized_tx,
                consensus_initialized_rx,
                consensus: OnceLock::new(),
                finality_tasks: Mutex::new(BTreeMap::default()),
                ui_task,
                app_task,
                weak_shared_modules,
                ui_pass_hash: std::sync::Mutex::new(ui_pass_hash),
                ui_pass_is_temporary: AtomicBool::new(ui_pass_is_temporary),
                peer_addr_needed: Arc::new(Notify::new()),
                node_app_ack_rx,
                node_app_ack_tx,
                pending_transactions_rx,
                app_consensus_module_init_consensus_version,
            };

            if let Some(consensus) = consensus {
                node.consensus
                    .set(Arc::new(consensus))
                    .ok()
                    .expect("Can't fail to set consensus");
            }

            node
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
    pub async fn consensus_init_static(
        db: Arc<Database>,
        root_secret: DeriveableSecret,
        extra_peers: Vec<PeerPubkey>,
        init_core_module_cons_version: ConsensusVersion,
    ) -> NodeInitResult<Consensus> {
        let pubkey = root_secret.get_peer_seckey()?.pubkey();

        let params = ConsensusParams {
            prev_mid_block: None,
            peers: [vec![pubkey], extra_peers].concat().into(),
            consensus_params_format_version: ConsensusParams::FORMAT_VERSION,
            init_core_module_cons_version,
            timestamp: Timestamp::now(),
            schedule_round: 0.into(),
            apply_round: 0.into(),
        };

        Ok(Consensus::init(&params, db, Some(pubkey), None).await?)
    }

    pub(crate) fn clone_strong(&self) -> Arc<Self> {
        self.handle_raw.upgrade().expect("Can't fail")
    }

    pub(crate) async fn make_iroh_endpoint(
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

    pub async fn consensus_join(&self, invite: &Invite) -> NodeJoinResult<()> {
        let consensus =
            Self::consensus_join_static(self.db().clone(), invite, self.peer_pubkey).await?;

        self.consensus
            .set(Arc::new(consensus))
            .ok()
            .expect("Can't be initialized twice");
        self.consensus_initialized_tx.send_replace(true);

        Ok(())
    }

    pub async fn consensus_init(&self, extra_peers: Vec<PeerPubkey>) -> NodeInitResult<()> {
        let Some(root_secret) = self.root_secret() else {
            return NoSecretSnafu.fail();
        };
        let consensus = Self::consensus_init_static(
            self.db().clone(),
            root_secret,
            extra_peers,
            self.app_consensus_module_init_consensus_version,
        )
        .await?;

        self.consensus
            .set(Arc::new(consensus))
            .ok()
            .expect("Can't be initialized twice");
        self.consensus_initialized_tx.send_replace(true);

        Ok(())
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

    pub(crate) fn get_peer_secret_expect(&self) -> PeerSeckey {
        self.root_secret
            .expect("Must contain root secret to participate")
            .get_peer_seckey()
            .expect("Level verified by now")
    }

    pub(crate) fn consensus(&self) -> Option<&Arc<Consensus>> {
        self.consensus.get()
    }

    pub(crate) fn consensus_expect(&self) -> &Arc<Consensus> {
        self.consensus
            .get()
            .expect("Must be called only when consensus is running")
    }
    pub(crate) async fn consensus_wait(&self) -> &Arc<Consensus> {
        let Ok(_) = self.consensus_initialized_rx.clone().wait_for(|r| *r).await else {
            future::pending().await
        };
        self.consensus
            .get()
            .expect("Must be called only when consensus is running")
    }

    pub async fn run(self: Arc<Self>) -> WhateverResult<()> {
        if !*self.consensus_initialized_rx.borrow() {
            info!(target: LOG_TARGET, "Waiting for consensus initialization via web UI");
            self.consensus_initialized_rx
                .clone()
                .wait_for(|rx| *rx)
                .await
                .whatever_context("Consensus init tx disconnected")?;
        }

        info!(
            target: LOG_TARGET,
            peer_pubkey = %self.peer_pubkey.fmt_option(),
            "Starting node…"
        );
        let invite = self.generate_invite_code().await?;
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

fn gen_random_pass() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect()
}
