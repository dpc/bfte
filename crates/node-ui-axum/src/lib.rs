// SPDX-License-Identifier: MIT

mod assets;
mod auth;
mod error;
mod misc;
mod page;
mod routes;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use assets::WithStaticRoutesExt as _;
use bfte_node_ui::NodeUiApi;
use bfte_util_error::WhateverResult;
use listenfd::ListenFd;
use routes::make_router;
use snafu::ResultExt as _;
use tokio::net::{TcpListener, TcpSocket};
use tower_http::CompressionLevel;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::SizeAbove;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};
use tracing::info;

const LOG_TARGET: &str = "bfte::node::ui";
const ROUTE_UI: &str = "/ui/";
const ROUTE_LOGIN: &str = "/ui/login";
const ROUTE_DS_CURRENT_ROUND: &str = "/datastar/current-round";

#[derive(Clone)]
pub(crate) struct UiState {
    pub(crate) node_api: NodeUiApi,
}
pub(crate) type ArcUiState = Arc<UiState>;

pub async fn run(node_api: NodeUiApi, bind_ui: SocketAddr) -> WhateverResult<Infallible> {
    let listener = get_listener(bind_ui, true).await?;

    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(time::Duration::minutes(2 * 24 * 60)));

    let router = make_router()
        .layer(session_layer)
        .with_static_routes()
        .with_state(Arc::new(UiState { node_api }));
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
