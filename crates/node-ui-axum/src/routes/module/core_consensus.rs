use maud::html;

use crate::UiState;

impl UiState {
    pub(crate) async fn render_consensus_module_page(
        &self,
        _module_id: bfte_consensus_core::module::ModuleId,
        consensus_module_ref: &bfte_module_core_consensus::CoreConsensusModule,
    ) -> maud::PreEscaped<String> {
        let module_configs = consensus_module_ref.get_modules_configs().await;
        let peer_set = consensus_module_ref.get_peer_set().await;
        html! {
            "Core consensus module"

            h3 { "Current Consensus Peers" }
            ul {
                @for peer in &peer_set {
                    li { (format!("{peer}")) }
                }
            }

            h3 { "Module Configs" }
            prev {
                (format!("{module_configs:?}"))
            }
        }
    }
}
