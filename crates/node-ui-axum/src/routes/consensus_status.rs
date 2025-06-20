use std::any::Any;

use async_stream::stream;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect};
use bfte_consensus_core::module::ModuleId;
use bfte_module_consensus_ctrl::ConsensusCtrlModule;
use bfte_util_error::WhateverResult;
use datastar::Sse;
use datastar::prelude::MergeSignals;
use maud::html;
use serde_json::json;
use snafu::{OptionExt as _, ResultExt as _};

use crate::error::{OtherSnafu, RequestResult};
use crate::misc::Maud;
use crate::page::NavbarSelector;
use crate::{ArcUiState, ROUTE_DS_CURRENT_ROUND, ROUTE_UI};

pub(crate) async fn root() -> Redirect {
    Redirect::permanent(ROUTE_UI)
}

async fn get_peer_set(
    state: &ArcUiState,
) -> WhateverResult<bfte_consensus_core::peer_set::PeerSet> {
    let module = state
        .modules
        .get_module(ModuleId::new(0))
        .await
        .whatever_context("Missing ConsensusCtrl module")?;
    let consensus_module_ref = (module.as_ref() as &dyn Any)
        .downcast_ref::<ConsensusCtrlModule>()
        .whatever_context("ConsensusCtrl module of the wrong kind")?;
    let peer_set = consensus_module_ref.get_peer_set().await;
    Ok(peer_set)
}

async fn get_peer_pubkey(state: &ArcUiState) -> WhateverResult<String> {
    let peer_pubkey = state.node_api.get_peer_pubkey()?;
    Ok(match peer_pubkey {
        Some(pubkey) => format!("{}", pubkey),
        None => "Not available".to_string(),
    })
}

async fn get_database_status(state: &ArcUiState) -> WhateverResult<(String, bool)> {
    let is_ephemeral = state.node_api.is_database_ephemeral()?;
    let status = if is_ephemeral {
        "In-memory (ephemeral)".to_string()
    } else {
        "Persistent".to_string()
    };
    Ok((status, is_ephemeral))
}

pub async fn get(state: State<ArcUiState>) -> RequestResult<impl IntoResponse> {
    let peer_set = get_peer_set(&state).await.unwrap_or_default();
    let peer_pubkey = get_peer_pubkey(&state)
        .await
        .unwrap_or_else(|_| "Not available".to_string());
    let (database_status, is_ephemeral) = get_database_status(&state)
        .await
        .unwrap_or_else(|_| ("Unknown".to_string(), false));

    let content = html! {
        div {
            h2 { "Overview" }

            section {
                h3 { "Status" }
                div
                    data-signals="{ round_and_timeout: '', finality_consensus: '', finality_self: '', node_app_ack: '' }"
                    data-on-load=(format!("@get('{}')", ROUTE_DS_CURRENT_ROUND)) {
                    div {
                        "Current Round: "
                        span data-text="$round_and_timeout" { "Loading..." }
                    }
                    div {
                        "Finality Vote: "
                        span data-text="$finality_self" { "Loading..." }
                    }
                    div {
                        "Finality Consensus: "
                        span data-text="$finality_consensus" { "Loading..." }
                    }
                    div {
                        "Node App Ack: "
                        span data-text="$node_app_ack" { "Loading..." }
                    }
                }
            }

            section {
                h3 { "Peers" }
                @if peer_set.is_empty() {
                    p { "No peers connected." }
                } @else {
                    ul {
                        @for peer in &peer_set {
                            li { (format!("{peer}")) }
                        }
                    }
                }
            }

            section {
                h3 { "Node Information" }
                p {
                    "Own peer public key: "
                    code style="word-break: break-all;" { (peer_pubkey) }
                }
                p {
                    "Database: "
                    @if is_ephemeral {
                        span style="color: red; font-weight: bold;" { (database_status) }
                    } @else {
                        span { (database_status) }
                    }
                }
            }

        }
    };
    Ok(Maud(
        state
            .render_html_page(Some(NavbarSelector::General), "Consensus Status", content)
            .await,
    ))
}

pub async fn updates(state: State<ArcUiState>) -> RequestResult<impl IntoResponse> {
    let mut round_and_timeout_rx = state
        .node_api
        .get_round_and_timeout_rx()
        .context(OtherSnafu)?;
    let mut finality_consensus_rx = state
        .node_api
        .get_finality_consensus_rx()
        .context(OtherSnafu)?;
    let mut finality_self_rx = state
        .node_api
        .get_finality_self_vote_rx()
        .context(OtherSnafu)?;
    let mut node_app_ack_rx = state.node_api.get_node_app_ack_rx().context(OtherSnafu)?;

    Ok(Sse(stream! {
        loop {
            let out = json! ({
                "round_and_timeout": format!("{} (timeout: {})", round_and_timeout_rx.borrow().0, round_and_timeout_rx.borrow().1),
                "finality_consensus": *finality_consensus_rx.borrow(),
                "finality_self": *finality_self_rx.borrow(),
                "node_app_ack": *node_app_ack_rx.borrow(),
            });

            yield MergeSignals::new(out.to_string());
            // TODO: workaround flushing bug
            yield MergeSignals::new(out.to_string());

            // Wait for changes on any of the channels
            tokio::select! {
                result = round_and_timeout_rx.changed() => {
                    if result.is_err() {
                        break;
                    }
                }
                result = finality_consensus_rx.changed() => {
                    if result.is_err() {
                        break;
                    }
                }
                result = finality_self_rx.changed() => {
                    if result.is_err() {
                        break;
                    }
                }
                result = node_app_ack_rx.changed() => {
                    if result.is_err() {
                        break;
                    }
                }
            }
        }
    }))
}
