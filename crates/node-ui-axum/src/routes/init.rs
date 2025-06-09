use axum::Form;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use bfte_invite::InviteString;
use maud::{Markup, html};
use serde::Deserialize;
use snafu::ResultExt as _;

use crate::error::{OtherSnafu, RequestResult};
use crate::fragments::labeled_input;
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
        state
            .node_api
            .consensus_join(&invite.into())
            .await
            .context(OtherSnafu)?;
    } else {
        state
            .node_api
            .consensus_init(vec![/* TODO */])
            .await
            .context(OtherSnafu)?;
    }
    Ok(Redirect::to(ROUTE_UI).into_response())
}

impl UiState {
    pub(crate) async fn render_consensus_init_page(&self) -> RequestResult<Markup> {
        let content = html! {
            section ."init-consensus-form" {
                div class="grid" {
                    // Create new consensus form
                    div {
                        article {
                            header {
                                h2 { "Create new consensus" }
                            }
                            form method="post" action=(ROUTE_INIT_CONSENSUS) {
                                p { "Start a new consensus network as the first node." }
                                button type="submit" { "Create" }
                            }
                        }
                    }

                    // Join existing consensus form
                    div {
                        article {
                            header {
                                h2 { "Join existing consensus" }
                            }
                            form method="post" action=(ROUTE_INIT_CONSENSUS) {
                                (
                                    labeled_input()
                                        .name("invite")
                                        .label("Invite code")
                                        .r#type("text")
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
