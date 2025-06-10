use bfte_module_meta::MetaModule;
use maud::html;

use crate::UiState;

impl UiState {
    pub(crate) async fn render_meta_module_page(
        &self,
        module_id: bfte_consensus_core::module::ModuleId,
        meta_module_ref: &MetaModule,
    ) -> maud::PreEscaped<String> {
        let consensus_values = meta_module_ref.get_consensus_values().await;

        html! {
            header {
                h1 { "Meta Module" }
                p { "Manage key-value consensus for meta information" }
            }

            section {
                h2 { "Current Consensus Values" }
                @if consensus_values.is_empty() {
                    p { "No consensus values set yet." }
                } @else {
                    table {
                        thead {
                            tr {
                                th { "Key" }
                                th { "Value" }
                            }
                        }
                        tbody {
                            @for (key, value) in &consensus_values {
                                tr {
                                    td { (format!("{}", key)) }
                                    td {
                                        @if let Ok(s) = std::str::from_utf8(value) {
                                            (s)
                                        } @else {
                                            (format!("0x{}", hex::encode(value.as_ref())))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            section {
                h2 { "Vote on Meta Keys" }
                p { "Select a key to vote on:" }

                ul {
                    @for key in 0u8..=255u8 {
                        li {
                            a href=(format!("/ui/module/{}/meta_key/{}", module_id, key))
                              class="button outline"
                              style="margin: 2px;" {
                                (format!("Key {}", key))
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) async fn render_meta_key_voting_page(
        &self,
        module_id: bfte_consensus_core::module::ModuleId,
        meta_module_ref: &MetaModule,
        key: u8,
    ) -> maud::PreEscaped<String> {
        let votes = meta_module_ref.get_votes_for_key(key).await;
        let consensus_values = meta_module_ref.get_consensus_values().await;
        let current_value = consensus_values.get(&key);

        html! {
            header {
                h1 { "Meta Key " (key) " Voting" }
                h2 {
                    a href=(format!("/ui/module/{}", module_id)) { "← Back to Meta Module" }
                }
            }

            @if let Some(current_value) = current_value {
                section {
                    h2 { "Current Consensus Value" }
                    div class="callout" {
                        strong { "Value: " }
                        @if let Ok(s) = std::str::from_utf8(current_value) {
                            (s)
                        } @else {
                            (format!("0x{}", hex::encode(current_value.as_ref())))
                        }
                    }
                }
            }

            section {
                h2 { "Current Votes" }
                @if votes.is_empty() {
                    p { "No votes cast yet for this key." }
                } @else {
                    table {
                        thead {
                            tr {
                                th { "Voter" }
                                th { "Voted Value" }
                            }
                        }
                        tbody {
                            @for (voter, value) in &votes {
                                tr {
                                    td {
                                        div { (format!("{}", voter.to_short())) }
                                        form method="post" action=(format!("/ui/module/{}/meta_key/{}/vote", module_id, key)) style="margin-top: 5px;" {
                                            input type="hidden" name="value" value={
                                                @if let Ok(s) = std::str::from_utf8(value) {
                                                    (s)
                                                } @else {
                                                    (format!("0x{}", hex::encode(value.as_ref())))
                                                }
                                            };
                                            input type="submit" value="Approve" class="button small";
                                        }
                                    }
                                    td {
                                        @if let Ok(s) = std::str::from_utf8(value) {
                                            (s)
                                        } @else {
                                            (format!("0x{}", hex::encode(value.as_ref())))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            section {
                h2 { "Submit Your Vote" }
                form method="post" action=(format!("/ui/module/{}/meta_key/{}/vote", module_id, key)) {
                    fieldset {
                        label for="value" { "Value to vote for:" }
                        input type="text" name="value" id="value" placeholder="Enter value (text or hex with 0x prefix)" required;
                        small { "You can enter plain text or hex values (prefix with 0x)" }
                    }
                    input type="submit" value="Submit Vote";
                }
            }
        }
    }
}
