mod app_consensus;

use std::any::Any;
use std::str::FromStr;

use axum::Form;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect};
use bfte_consensus_core::module::ModuleId;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_module_app_consensus::AppConsensusModule;
use bfte_util_error::fmt::FmtCompact as _;
use maud::{Markup, html};
use serde::Deserialize;
use snafu::ResultExt as _;
use tracing::warn;

use crate::error::{RequestResult, UserSnafu};
use crate::misc::Maud;
use crate::page::NavbarSelector;
use crate::{ArcUiState, LOG_TARGET, UiState};

#[axum::debug_handler]
pub async fn get(
    Path(module_id): Path<ModuleId>,
    state: State<ArcUiState>,
) -> RequestResult<impl IntoResponse> {
    let content = state.render_module_page(module_id).await;
    Ok(Maud(
        state
            .render_html_page(Some(NavbarSelector::Module(module_id)), "Hello!", content)
            .await,
    ))
}

#[derive(Deserialize)]
pub struct AddPeerVoteForm {
    peer_pubkey: String,
}

#[derive(Deserialize)]
pub struct RemovePeerVoteForm {
    peer_pubkey: String,
}

#[axum::debug_handler]
pub async fn post_add_peer_vote(
    Path(module_id): Path<ModuleId>,
    state: State<ArcUiState>,
    Form(form): Form<AddPeerVoteForm>,
) -> RequestResult<impl IntoResponse> {
    let Some(module) = state.modules.get_module(module_id).await else {
        return Ok(Redirect::to(&format!("/ui/module/{module_id}")).into_response());
    };

    if module.config.kind == bfte_module_app_consensus::KIND {
        let Some(consensus_module_ref) =
            (module.inner.as_ref() as &dyn Any).downcast_ref::<AppConsensusModule>()
        else {
            return Ok(Redirect::to(&format!("/ui/module/{module_id}")).into_response());
        };

        let peer_pubkey = PeerPubkey::from_str(&form.peer_pubkey)
            .whatever_context("Failed to deserialize pubkey")
            .context(UserSnafu)?;
        consensus_module_ref
                .set_pending_add_peer_vote(peer_pubkey)
                .await.inspect_err(|err| {
                    warn!(target: LOG_TARGET, err = %err.fmt_compact(), "Could not submit add peer vote");
                }).context(UserSnafu)?;
    }
    Ok(Redirect::to(&format!("/ui/module/{module_id}")).into_response())
}

#[axum::debug_handler]
pub async fn post_remove_peer_vote(
    Path(module_id): Path<ModuleId>,
    state: State<ArcUiState>,
    Form(form): Form<RemovePeerVoteForm>,
) -> RequestResult<impl IntoResponse> {
    let Some(module) = state.modules.get_module(module_id).await else {
        return Ok(Redirect::to(&format!("/ui/module/{module_id}")).into_response());
    };

    if module.config.kind == bfte_module_app_consensus::KIND {
        let Some(consensus_module_ref) =
            (module.inner.as_ref() as &dyn Any).downcast_ref::<AppConsensusModule>()
        else {
            return Ok(Redirect::to(&format!("/ui/module/{module_id}")).into_response());
        };

        let peer_pubkey = PeerPubkey::from_str(&form.peer_pubkey)
            .whatever_context("Failed to deserialize pubkey")
            .context(UserSnafu)?;
        consensus_module_ref
                .set_pending_remove_peer_vote(peer_pubkey)
                .await.inspect_err(|err| {
                    warn!(target: LOG_TARGET, err = %err.fmt_compact(), "Could not submit remove peer vote");
                }).context(UserSnafu)?;
    }

    Ok(Redirect::to(&format!("/ui/module/{module_id}")).into_response())
}

impl UiState {
    async fn render_module_page(&self, module_id: ModuleId) -> Markup {
        let Some(module) = self.modules.get_module(module_id).await else {
            return html! { "Module instance does not exist" };
        };

        match module.config.kind {
            bfte_module_app_consensus::KIND => {
                let Some(consensus_module_ref) =
                    (module.inner.as_ref() as &dyn Any).downcast_ref::<AppConsensusModule>()
                else {
                    return html! { "Module instance is not a recognized consensus module" };
                };

                self.render_consensus_module_page(module_id, consensus_module_ref)
                    .await
            }
            kind => html! {
                (format!("TBD. Generic handling of module {module_id} of kind {}", kind))
            },
        }
    }
}
