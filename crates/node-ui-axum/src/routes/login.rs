use axum::Form;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use maud::{Markup, html};
use serde::Deserialize;
use snafu::{OptionExt as _, ResultExt as _};
use tower_sessions::Session;

use crate::error::{LoginRequiredSnafu, OtherSnafu, RequestResult};
use crate::fragments::labeled_input;
use crate::middleware::{SESSION_KEY, UserAuth};
use crate::misc::Maud;
use crate::{ArcUiState, ROUTE_LOGIN, ROUTE_UI, UiState};

#[derive(Deserialize)]
pub struct GetInput {
    redirect: Option<String>,
}
pub async fn get(
    state: State<ArcUiState>,
    Query(query): Query<GetInput>,
) -> RequestResult<impl IntoResponse> {
    Ok(Maud(state.render_login_page(query.redirect).await?).into_response())
}

#[derive(Deserialize)]
pub struct Input {
    temp_password: Option<String>,
    password: String,
    redirect: Option<String>,
}

pub async fn post(
    state: State<ArcUiState>,
    session: Session,
    Form(form): Form<Input>,
) -> RequestResult<Response> {
    let cur_pass_is_temporary = state
        .node_api
        .is_ui_password_temporary()
        .context(OtherSnafu)?;

    if cur_pass_is_temporary {
        let temp_pass = form
            .temp_password
            .whatever_context("temporary password missing")
            .context(OtherSnafu)?;
        if blake3::hash(temp_pass.trim().as_bytes())
            != state.node_api.get_ui_password_hash().context(OtherSnafu)?
        {
            return (LoginRequiredSnafu {
                path: form.redirect,
            })
            .fail();
        }

        state
            .node_api
            .change_ui_password(&form.password)
            .await
            .context(OtherSnafu)?;
    } else {
        if blake3::hash(form.password.trim().as_bytes())
            != state.node_api.get_ui_password_hash().context(OtherSnafu)?
        {
            return (LoginRequiredSnafu {
                path: form.redirect,
            })
            .fail();
        }
    }

    session
        .insert(SESSION_KEY, &UserAuth)
        .await
        .whatever_context("Could not create session")
        .context(OtherSnafu)?;

    let redirect_url = form.redirect.as_deref().unwrap_or(ROUTE_UI);
    Ok(Redirect::to(redirect_url).into_response())
}

impl UiState {
    pub(crate) async fn render_login_page(
        &self,
        redirect: Option<String>,
    ) -> RequestResult<Markup> {
        let pass_is_temporary = self
            .node_api
            .is_ui_password_temporary()
            .context(OtherSnafu)?;

        let content = html! {
            section ."login-form" {
                header {
                    h2 { "Sign in" }
                }
                form method="post" action=(ROUTE_LOGIN) {
                    @if let Some(ref redirect_url) = redirect {
                        input type="hidden" name="redirect" value=(redirect_url);
                    }
                    @if pass_is_temporary {
                        (
                            labeled_input()
                                .name("temp_password")
                                .label("Temporary Password")
                                .r#type("password")
                                .placeholder("Enter temporary password (check logs)")
                                .required(true)
                                .call()
                        )
                    }
                    (
                        labeled_input()
                            .name("password")
                            .label(if pass_is_temporary { "Set Password" } else { "Password" })
                            .r#type("password")
                            .placeholder(if pass_is_temporary { "Enter new password" } else { "Enter your password" })
                            .required(true)
                            .call()
                    )
                    button type="submit" data-on-click=(format!("@post({}, {{contentType: 'form'}})", ROUTE_LOGIN)) {
                        "Sign in"
                    }
                }
            }
        };
        Ok(self.render_html_page(None, "Sign in", content).await)
    }
}
