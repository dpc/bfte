use std::time::Duration;

use async_stream::stream;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect};
use datastar::Sse;
use datastar::prelude::MergeSignals;
use maud::html;
use serde_json::json;
use snafu::ResultExt as _;
use tokio::time::sleep;

use crate::error::{OtherSnafu, RequestResult};
use crate::misc::Maud;
use crate::page::NavbarSelector;
use crate::{ArcUiState, ROUTE_DS_CURRENT_ROUND, ROUTE_UI};

pub(crate) async fn root() -> Redirect {
    Redirect::permanent(ROUTE_UI)
}

pub async fn get(state: State<ArcUiState>) -> RequestResult<impl IntoResponse> {
    let content = html! {
        "Hello!"
        input data-bind-input;
        div {
            "The text is: " span data-text="$input" {}
        }

        div
            data-signals="{ cur_round: '' }"
            data-text="$cur_round"
            data-on-load=(format!("@get('{}')", ROUTE_DS_CURRENT_ROUND)) {

            "Current round"
        }
    };
    Ok(Maud(
        state
            .render_html_page(Some(NavbarSelector::Consensus), "Hello!", content)
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

            sleep(Duration::from_secs(1)).await;

            if current_round_rx.changed().await.is_err() {
                break;
            }
        }
    }))
}
