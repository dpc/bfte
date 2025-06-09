use std::collections::BTreeMap;

use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_consensus_core::ver::{ConsensusVersion, ConsensusVersionMajor, ConsensusVersionMinor};
use bfte_module::module::config::ModuleConfig;
use maud::{Markup, html};

use crate::UiState;

fn get_module_kind_name(kind: ModuleKind) -> Option<&'static str> {
    match kind {
        k if k == bfte_module_app_consensus::KIND => Some("App Consensus"),
        k if k == bfte_module_meta::KIND => Some("Meta"),
        _ => None,
    }
}

impl UiState {
    pub(crate) async fn render_consensus_module_page(
        &self,
        _module_id: bfte_consensus_core::module::ModuleId,
        consensus_module_ref: &bfte_module_app_consensus::AppConsensusModule,
    ) -> maud::PreEscaped<String> {
        let module_configs = consensus_module_ref.get_modules_configs().await;
        let peer_set = consensus_module_ref.get_peer_set().await;
        let add_peer_votes = consensus_module_ref.get_add_peer_votes().await;
        let remove_peer_votes = consensus_module_ref.get_remove_peer_votes().await;
        let add_module_votes = consensus_module_ref.get_add_module_votes().await;
        html! {
            header {
                h1 { "App Consensus" }
            }

            h2 { "Membership" }

            section {
                h3 { "Current Peers" }
                ul {
                    @for peer in &peer_set {
                        li { (format!("{peer}")) }
                    }
                }
            }

            section {
                h3 { "Add Peer" }
                @if !add_peer_votes.is_empty() {
                    h4 { "Pending Votes:" }
                    ul {
                        @for (voter, voted_for) in &add_peer_votes {
                            li { (format!("{} → {}", voter, voted_for)) }
                        }
                    }
                }
                form method="post" action=(format!("/ui/module/{}/add_peer_vote", _module_id)) {
                    fieldset role="group" {
                        input type="text" name="peer_pubkey" placeholder="Peer's public key" required;
                        input type="submit" value="Add";
                    }
                }
            }

            section {
                h3 { "Remove Peer" }
                @if !remove_peer_votes.is_empty() {
                    h4 { "Pending Votes:" }
                    ul {
                        @for (voter, voted_for) in &remove_peer_votes {
                            li { (format!("{} → {}", voter, voted_for)) }
                        }
                    }
                }
                form method="post" action=(format!("/ui/module/{}/remove_peer_vote", _module_id)) {
                    fieldset role="group" {
                        input type="text" name="peer_pubkey" placeholder="Peer's public key" required;
                        input type="submit" value ="Remove";
                    }
                }
            }

            h2 { "Modules" }

            section {
                h3 { "Active modules" }
                table {
                    thead {
                        tr {
                            th { "Module ID" }
                            th { "Module Kind" }
                            th { "Consensus Version" }
                        }
                    }
                    tbody {
                        @for (module_id, config) in &module_configs {
                            tr {
                                td { (format!("{module_id}")) }
                                td {
                                    @if let Some(kind_name) = get_module_kind_name(config.kind) {
                                        (kind_name)
                                    } @else {
                                        (format!("{}", config.kind))
                                    }
                                }
                                td { (format!("{}", config.version)) }
                            }
                        }
                    }
                }
            }

            section {
                h3 { "Add Module" }
                @if !add_module_votes.is_empty() {
                    h4 { "Pending Votes:" }
                    ul {
                        @for (voter, (module_kind, consensus_version)) in &add_module_votes {
                            li {
                                @let module_name = get_module_kind_name(*module_kind).unwrap_or("Unknown");
                                (format!("{} → {} (v{})", voter, module_name, consensus_version))
                            }
                        }
                    }
                }
                (self.render_add_module_form(_module_id, &module_configs).await)
            }
        }
    }

    async fn render_add_module_form(
        &self,
        module_id: ModuleId,
        existing_modules: &BTreeMap<ModuleId, ModuleConfig>,
    ) -> Markup {
        // Get existing module kinds
        let existing_kinds: std::collections::HashSet<ModuleKind> = existing_modules
            .values()
            .map(|config| config.kind)
            .collect();

        // Filter available module kinds
        let available_kinds: Vec<(ModuleKind, &str, u16, u16)> = self
            .modules_inits
            .iter()
            .filter_map(|(kind, init)| {
                // Skip singleton modules that already exist
                if init.singleton() && existing_kinds.contains(kind) {
                    return None;
                }

                let display_name = get_module_kind_name(*kind).unwrap_or("Unknown");
                let supported_versions = init.supported_versions();

                // Get the latest supported version (highest major, then highest minor)
                if let Some((&major, &minor)) = supported_versions.iter().max() {
                    // Since these are tuple structs, we need to access the inner value
                    // For now, let's use the Display trait since they derive From which includes
                    // Display
                    let major_str = format!("{}", major);
                    let minor_str = format!("{}", minor);
                    let major_val: u16 = major_str.parse().unwrap_or(0);
                    let minor_val: u16 = minor_str.parse().unwrap_or(0);
                    Some((*kind, display_name, major_val, minor_val))
                } else {
                    None
                }
            })
            .collect();

        if available_kinds.is_empty() {
            html! {
                p { "No modules available to add." }
            }
        } else {
            html! {
                form method="post" action=(format!("/ui/module/{}/add_module_vote", module_id)) {
                    fieldset role="group" {
                        select name="module_kind" required {
                            option value="" { "Select module to add..." }
                            @for (kind, display_name, major, minor) in &available_kinds {
                                option value=(format!("{}:{}.{}", kind, major, minor)) {
                                    (format!("{} (v{}.{})", display_name, major, minor))
                                }
                            }
                        }
                        input type="submit" value="Add Module";
                    }
                }
            }
        }
    }
}
