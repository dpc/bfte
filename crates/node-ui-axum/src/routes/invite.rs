use axum::extract::State;
use axum::response::IntoResponse;
use bfte_util_error::WhateverResult;
use maud::html;

use crate::ArcUiState;
use crate::error::RequestResult;
use crate::misc::Maud;
use crate::page::NavbarSelector;

async fn get_invite_code(state: &ArcUiState) -> WhateverResult<String> {
    let invite = state.node_api.generate_invite_code().await?;
    Ok(format!("{}", invite))
}

pub async fn get(state: State<ArcUiState>) -> RequestResult<impl IntoResponse> {
    let invite_code = get_invite_code(&state)
        .await
        .unwrap_or_else(|_| "Not available".to_string());

    let content = html! {
        div {
            h2 { "Invite Code" }

            section {
                p {
                    "Share this code with others to invite them to join the consensus:"
                }
                code style="display: block; padding: 1em; background: var(--pico-code-background-color); word-break: break-all; margin: 1em 0;" {
                    (invite_code)
                }
                p ."text-muted" {
                    "Note: This invite code allows other nodes to join your consensus network. Keep it secure and only share with trusted parties."
                }
            }
        }
    };

    Ok(Maud(
        state
            .render_html_page(Some(NavbarSelector::General), "Invite Code", content)
            .await,
    ))
}
