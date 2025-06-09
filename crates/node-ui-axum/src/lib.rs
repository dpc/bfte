// SPDX-License-Identifier: MIT

mod assets;
mod error;
mod fragments;
mod middleware;
mod misc;
mod page;
mod routes;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use assets::WithStaticRoutesExt as _;
use axum::Extension;
use bfte_consensus_core::module::ModuleKind;
use bfte_module::module::DynModuleInit;
use bfte_node_shared_modules::WeakSharedModules;
use bfte_node_ui::NodeUiApi;
use bfte_util_error::WhateverResult;
use listenfd::ListenFd;
use routes::make_router;
use snafu::ResultExt as _;
use tokio::net::{TcpListener, TcpSocket};
use tower::ServiceBuilder;
use tower_http::CompressionLevel;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::SizeAbove;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};
use tracing::info;

const LOG_TARGET: &str = "bfte::node::ui";
const ROUTE_UI: &str = "/ui/";
const ROUTE_LOGIN: &str = "/ui/login";
const ROUTE_MODULE: &str = "/ui/module/{module-id}";
const ROUTE_MODULE_ADD_PEER_VOTE: &str = "/ui/module/{module-id}/add_peer_vote";
const ROUTE_MODULE_REMOVE_PEER_VOTE: &str = "/ui/module/{module-id}/remove_peer_vote";
const ROUTE_INIT_CONSENSUS: &str = "/ui/init";
const ROUTE_INVITE: &str = "/ui/invite";
const ROUTE_DS_CURRENT_ROUND: &str = "/datastar/current-round";

#[derive(Clone)]
pub(crate) struct UiState {
    pub(crate) node_api: NodeUiApi,
    pub(crate) modules: WeakSharedModules,
    pub(crate) modules_inits: BTreeMap<ModuleKind, DynModuleInit>,
}
pub(crate) type ArcUiState = Arc<UiState>;

pub async fn run(
    node_api: NodeUiApi,
    bind_ui: SocketAddr,
    shared_modules: WeakSharedModules,
    modules_inits: BTreeMap<ModuleKind, DynModuleInit>,
) -> WhateverResult<Infallible> {
    let listener = get_listener(bind_ui, true).await?;

    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(time::Duration::minutes(2 * 24 * 60)));

    let state = Arc::new(UiState {
        node_api,
        modules: shared_modules,
        modules_inits,
    });
    let router = make_router()
        .layer(
            ServiceBuilder::new()
                .layer(Extension(state.clone()))
                .layer(axum::middleware::from_fn(middleware::cache_control))
                .layer(axum::middleware::from_fn(middleware::require_auth))
                .layer(axum::middleware::from_fn(middleware::consensus_init)),
        )
        .layer(session_layer)
        .with_static_routes()
        .with_state(state);
    let listen = listener
        .local_addr()
        .whatever_context("Failed to get local addr")?;

    info!(
        target: LOG_TARGET,
        addr = %listen,
        // origin = %self.opts.cors_origin_url_str(listen),
        "Starting web UI server..."
    );

    axum::serve(
        listener,
        router
            .layer(compression_layer())
            .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .whatever_context("Failed to run axum server")?;

    todo!()
}

pub async fn get_listener(bind_ui: SocketAddr, reuseport: bool) -> WhateverResult<TcpListener> {
    if let Some(listener) = ListenFd::from_env()
        .take_tcp_listener(0)
        .whatever_context("Failed to take listenfd tcp listener")?
    {
        listener
            .set_nonblocking(true)
            .whatever_context("Failed to set socket to non-blocking")?;
        return TcpListener::from_std(listener)
            .whatever_context("Failed to convert listenfd listener");
    }
    let socket = {
        let addr = bind_ui;

        let socket = if addr.is_ipv4() {
            TcpSocket::new_v4().whatever_context("Failed to get tcpv4 socket")?
        } else {
            TcpSocket::new_v6().whatever_context("Failed to get tcpv6 socket")?
        };
        if reuseport {
            #[cfg(unix)]
            socket
                .set_reuseport(true)
                .whatever_context("Failed to set reuseport")?;
        }
        socket
            .set_nodelay(true)
            .whatever_context("Failed to set nodelay")?;

        socket.bind(addr).whatever_context("Failed to bind")?;

        socket
    };

    socket
        .listen(1024)
        .whatever_context("Failed to listen on socket")
}

fn compression_layer() -> CompressionLayer<SizeAbove> {
    CompressionLayer::new()
        .quality(CompressionLevel::Fastest)
        .br(true)
        .compress_when(SizeAbove::new(512))
}
