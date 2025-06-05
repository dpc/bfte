mod core_consensus;

use std::any::Any;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use bfte_consensus_core::module::ModuleId;
use bfte_module_core_consensus::CoreConsensusModule;
use maud::{Markup, html};

use crate::error::RequestResult;
use crate::misc::Maud;
use crate::page::NavbarSelector;
use crate::{ArcUiState, UiState};

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

impl UiState {
    async fn render_module_page(&self, module_id: ModuleId) -> Markup {
        let Some(module) = self.modules.get_module(module_id).await else {
            return html! { "Module instance does not exist" };
        };

        match module.config.kind {
            bfte_module_core_consensus::KIND => {
                let Some(consensus_module_ref) =
                    (module.inner.as_ref() as &dyn Any).downcast_ref::<CoreConsensusModule>()
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
