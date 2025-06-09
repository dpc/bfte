use std::any::Any;

use async_stream::stream;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect};
use bfte_consensus_core::module::ModuleId;
use bfte_module_app_consensus::AppConsensusModule;
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

async fn get_peer_count(state: &ArcUiState) -> WhateverResult<usize> {
    let module = state
        .modules
        .get_module(ModuleId::new(0))
        .await
        .whatever_context("Missing AppConsensus module")?;
    let consensus_module_ref = (module.as_ref() as &dyn Any)
        .downcast_ref::<AppConsensusModule>()
        .whatever_context("AppConsensus module of the wrong kind")?;
    let peer_set = consensus_module_ref.get_peer_set().await;
    Ok(peer_set.len())
}

async fn get_invite_code(state: &ArcUiState) -> WhateverResult<String> {
    let invite = state.node_api.generate_invite_code().await?;
    Ok(format!("{}", invite))
}

pub async fn get(state: State<ArcUiState>) -> RequestResult<impl IntoResponse> {
    let peer_count = get_peer_count(&state).await.unwrap_or(0);
    let invite_code = get_invite_code(&state)
        .await
        .unwrap_or_else(|_| "Not available".to_string());

    let content = html! {
        div {
            h2 { "Consensus Status" }

            section {
                h3 { "Current Status" }
                div
                    data-signals="{ round_and_timeout: '', finality_consensus: '', finality_self: '', node_app_ack: '' }"
                    data-on-load=(format!("@get('{}')", ROUTE_DS_CURRENT_ROUND)) {
                    div {
                        "Current Round: "
                        span data-text="$round_and_timeout" { "Loading..." }
                    }
                    div {
                        "Finality Consensus: "
                        span data-text="$finality_consensus" { "Loading..." }
                    }
                    div {
                        "Finality Self: "
                        span data-text="$finality_self" { "Loading..." }
                    }
                    div {
                        "Node App Ack: "
                        span data-text="$node_app_ack" { "Loading..." }
                    }
                }
            }

            section {
                h3 { "Peer Information" }
                p { "Number of peers: " (peer_count) }
            }

            section {
                h3 { "Invite Code" }
                p {
                    "Share this code with others to invite them to join the consensus:"
                }
                code style="display: block; padding: 1em; background: var(--pico-code-background-color); word-break: break-all;" {
                    (invite_code)
                }
            }
        }
    };
    Ok(Maud(
        state
            .render_html_page(Some(NavbarSelector::Consensus), "Consensus Status", content)
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
