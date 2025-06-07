use bfte_consensus_core::module::ModuleKind;
use maud::html;

use crate::UiState;

fn get_module_kind_name(kind: ModuleKind) -> Option<&'static str> {
    match kind {
        k if k == bfte_module_app_consensus::KIND => Some("App Consensus"),
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
        html! {
            header {
                h1 { "App Consensus" }
            }

            h2 { "Membership" }

            section {
                h3 { "Consensus Peers" }
                ul {
                    @for peer in &peer_set {
                        li { (format!("{peer}")) }
                    }
                }
            }

            section {
                h3 { "Add Peer" }
                @if !add_peer_votes.is_empty() {
                    h4 { "Current Add Peer Votes:" }
                    ul {
                        @for (voter, voted_for) in &add_peer_votes {
                            li { (format!("{} → {}", voter, voted_for)) }
                        }
                    }
                }
                form method="post" action=(format!("/ui/module/{}/add_peer_vote", _module_id)) {
                    fieldset {
                        label {
                            "Peer Public Key"
                            input type="text" name="peer_pubkey" placeholder="Enter peer public key" required;
                        }
                        button type="submit" { "Vote to Add Peer" }
                    }
                }
            }

            section {
                h3 { "Remove Peer" }
                @if !remove_peer_votes.is_empty() {
                    h4 { "Current Remove Peer Votes:" }
                    ul {
                        @for (voter, voted_for) in &remove_peer_votes {
                            li { (format!("{} → {}", voter, voted_for)) }
                        }
                    }
                }
                form method="post" action=(format!("/ui/module/{}/remove_peer_vote", _module_id)) {
                    fieldset {
                        label {
                            "Peer Public Key"
                            input type="text" name="peer_pubkey" placeholder="Enter peer public key" required;
                        }
                        button type="submit" { "Vote to Remove Peer" }
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
        }
    }
}
