mod app_consensus;
mod meta;

use std::any::Any;
use std::str::FromStr;
use std::sync::Arc;

use axum::Form;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect};
use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_consensus_core::peer::PeerPubkey;
use bfte_consensus_core::ver::{ConsensusVersion, ConsensusVersionMajor, ConsensusVersionMinor};
use bfte_module_app_consensus::AppConsensusModule;
use bfte_module_meta::MetaModule;
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

#[derive(Deserialize)]
pub struct AddModuleVoteForm {
    module_kind: String, // Format: "kind:major.minor"
}

#[derive(Deserialize)]
pub struct MetaVoteForm {
    value: String,
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

#[axum::debug_handler]
pub async fn post_add_module_vote(
    Path(module_id): Path<ModuleId>,
    state: State<ArcUiState>,
    Form(form): Form<AddModuleVoteForm>,
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

        // Parse the module_kind string format: "kind:major.minor"
        let parts: Vec<&str> = form.module_kind.split(':').collect();
        if parts.len() != 2 {
            warn!(target: LOG_TARGET, "Invalid module_kind format: {}", form.module_kind);
            return Ok(Redirect::to(&format!("/ui/module/{module_id}")).into_response());
        }

        let module_kind_value: u32 = parts[0]
            .parse()
            .whatever_context("Failed to parse module kind")
            .context(UserSnafu)?;
        let module_kind = ModuleKind::new(module_kind_value);

        let version_parts: Vec<&str> = parts[1].split('.').collect();
        if version_parts.len() != 2 {
            warn!(target: LOG_TARGET, "Invalid version format: {}", parts[1]);
            return Ok(Redirect::to(&format!("/ui/module/{module_id}")).into_response());
        }

        let major_value: u16 = version_parts[0]
            .parse()
            .whatever_context("Failed to parse major version")
            .context(UserSnafu)?;
        let minor_value: u16 = version_parts[1]
            .parse()
            .whatever_context("Failed to parse minor version")
            .context(UserSnafu)?;

        let major = ConsensusVersionMajor::new(major_value);
        let minor = ConsensusVersionMinor::new(minor_value);

        let consensus_version = ConsensusVersion::new(major, minor);

        consensus_module_ref
            .set_pending_add_module_vote(module_kind, consensus_version)
            .await
            .inspect_err(|err| {
                warn!(target: LOG_TARGET, err = %err.fmt_compact(), "Could not submit add module vote");
            })
            .context(UserSnafu)?;
    }

    Ok(Redirect::to(&format!("/ui/module/{module_id}")).into_response())
}

#[axum::debug_handler]
pub async fn get_meta_key(
    Path((module_id, key)): Path<(ModuleId, u8)>,
    state: State<ArcUiState>,
) -> RequestResult<impl IntoResponse> {
    let Some(module) = state.modules.get_module(module_id).await else {
        return Ok(Redirect::to(&format!("/ui/module/{module_id}")).into_response());
    };

    if module.config.kind == bfte_module_meta::KIND {
        let Some(meta_module_ref) =
            (module.inner.as_ref() as &dyn Any).downcast_ref::<MetaModule>()
        else {
            return Ok(Redirect::to(&format!("/ui/module/{module_id}")).into_response());
        };

        let content = state
            .render_meta_key_voting_page(module_id, meta_module_ref, key)
            .await;
        return Ok(Maud(
            state
                .render_html_page(
                    Some(NavbarSelector::Module(module_id)),
                    &format!("Meta Key {}", key),
                    content,
                )
                .await,
        )
        .into_response());
    }

    Ok(Redirect::to(&format!("/ui/module/{module_id}")).into_response())
}

#[axum::debug_handler]
pub async fn post_meta_vote(
    Path((module_id, key)): Path<(ModuleId, u8)>,
    state: State<ArcUiState>,
    Form(form): Form<MetaVoteForm>,
) -> RequestResult<impl IntoResponse> {
    let Some(module) = state.modules.get_module(module_id).await else {
        return Ok(Redirect::to(&format!("/ui/module/{module_id}/meta_key/{key}")).into_response());
    };

    if module.config.kind == bfte_module_meta::KIND {
        let Some(meta_module_ref) =
            (module.inner.as_ref() as &dyn Any).downcast_ref::<MetaModule>()
        else {
            return Ok(
                Redirect::to(&format!("/ui/module/{module_id}/meta_key/{key}")).into_response(),
            );
        };

        // Parse value - either hex (0x prefix) or plain text
        let value_bytes: Arc<[u8]> = if form.value.starts_with("0x") || form.value.starts_with("0X")
        {
            // Parse as hex
            let hex_str = &form.value[2..];
            match hex::decode(hex_str) {
                Ok(bytes) => bytes.into(),
                Err(_) => {
                    warn!(target: LOG_TARGET, "Invalid hex value: {}", form.value);
                    return Ok(
                        Redirect::to(&format!("/ui/module/{module_id}/meta_key/{key}"))
                            .into_response(),
                    );
                }
            }
        } else {
            // Treat as plain text
            form.value.into_bytes().into()
        };

        meta_module_ref
            .propose_key_value(key, value_bytes)
            .await
            .inspect_err(|err| {
                warn!(target: LOG_TARGET, err = %err.fmt_compact(), "Could not submit meta vote");
            })
            .context(UserSnafu)?;
    }

    Ok(Redirect::to(&format!("/ui/module/{module_id}/meta_key/{key}")).into_response())
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
            bfte_module_meta::KIND => {
                let Some(meta_module_ref) =
                    (module.inner.as_ref() as &dyn Any).downcast_ref::<MetaModule>()
                else {
                    return html! { "Module instance is not a recognized meta module" };
                };

                self.render_meta_module_page(module_id, meta_module_ref)
                    .await
            }
            kind => html! {
                (format!("TBD. Generic handling of module {module_id} of kind {}", kind))
            },
        }
    }
}
