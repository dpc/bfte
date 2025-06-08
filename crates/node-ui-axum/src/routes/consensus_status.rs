use std::any::Any;
use std::time::Duration;

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
use tokio::time::sleep;

use crate::error::{InternalServerSnafu, OtherSnafu, RequestResult};
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
                h3 { "Current Round" }
                div
                    data-signals="{ cur_round: '' }"
                    data-text="$cur_round"
                    data-on-load=(format!("@get('{}')", ROUTE_DS_CURRENT_ROUND)) {
                    "Loading..."
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

pub async fn current_round(state: State<ArcUiState>) -> RequestResult<impl IntoResponse> {
    let mut current_round_rx = state
        .node_api
        .get_round_and_timeout_rx()
        .context(OtherSnafu)?;
    Ok(Sse(stream! {
        loop {
            let out = json! ({
                "cur_round": current_round_rx.borrow().0,
            });

            yield MergeSignals::new(out.to_string());
            // TODO: workaround flushing bug
            yield MergeSignals::new(out.to_string());

            sleep(Duration::from_secs(1)).await;

            if current_round_rx.changed().await.is_err() {
                break;
            }
        }
    }))
}
