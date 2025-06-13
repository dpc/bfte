use std::time::Duration;

use axum::Form;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use bfte_invite::InviteString;
use maud::{Markup, html};
use serde::Deserialize;
use snafu::ResultExt as _;
use tokio::time::timeout;

use crate::error::{ConsensusCreateSnafu, ConsensusJoinSnafu, OtherSnafu, RequestResult};
use crate::fragments::labeled_textarea;
use crate::misc::Maud;
use crate::{ArcUiState, ROUTE_INIT_CONSENSUS, ROUTE_UI, UiState};

pub async fn get(state: State<ArcUiState>) -> RequestResult<impl IntoResponse> {
    Ok(Maud(state.render_consensus_init_page().await?).into_response())
}

#[derive(Deserialize)]
pub struct Input {
    // if invite code is set, we join, otherwise we create
    invite: Option<InviteString>,
}

pub async fn post(state: State<ArcUiState>, Form(form): Form<Input>) -> RequestResult<Response> {
    if let Some(invite) = form.invite {
        timeout(
            Duration::from_secs(30),
            state.node_api.consensus_join(&invite.into()),
        )
        .await
        .whatever_context("Timeout")
        .context(ConsensusJoinSnafu)?
        .context(ConsensusJoinSnafu)?;
    } else {
        state
            .node_api
            .consensus_init(vec![/* TODO */])
            .await
            .context(ConsensusCreateSnafu)?;
    }
    Ok(Redirect::to(ROUTE_UI).into_response())
}

impl UiState {
    pub(crate) async fn render_consensus_init_page(&self) -> RequestResult<Markup> {
        let has_secret = self.node_api.has_root_secret().context(OtherSnafu)?;
        let is_db_ephemeral = self.node_api.is_database_ephemeral().context(OtherSnafu)?;
        let can_create_consensus = has_secret && !is_db_ephemeral;
        let content = html! {
            section ."init-consensus-form" {
                div class="grid" {
                    // Create new consensus form
                    div {
                        article {
                            header {
                                h2 { "Create new consensus" }
                            }
                            div role="status" {
                                p id="error-response-form-create";
                            }
                            form
                                method="post"
                                action=(ROUTE_INIT_CONSENSUS)
                                x-target="_top"
                                "x-target.error"="error-response-form-create:error-response"
                            {
                                p { "Start a new consensus network as the first node." }
                                button type="submit" disabled[(!can_create_consensus)] { "Create" }
                                @if !has_secret {
                                    p { "Must have a root secret set with" code { "--secret-path" } "." }
                                }
                                @if is_db_ephemeral {
                                    p { "Must set location for a persistent database set with" code { "--data-dir" } "."}
                                }
                            }
                        }
                    }

                    // Join existing consensus form
                    div {
                        article {
                            header {
                                h2 { "Join existing consensus" }
                            }
                            div role="status" {
                                p id="error-response-form-join";
                            }
                            form
                                method="post"
                                action=(ROUTE_INIT_CONSENSUS)
                                x-target="_top"
                                "x-target.error"="error-response-form-join:error-response"
                            {
                                (
                                    labeled_textarea()
                                        .name("invite")
                                        .label(
                                            "Join an existing consensus as a non-voting node with an invite code"
                                        )
                                        .required(true)
                                        .call()
                                )
                                button type="submit" { "Join" }
                            }
                        }
                    }
                }
            }
        };
        Ok(self
            .render_html_page(None, "Initialize Consensus", content)
            .await)
    }
}
