use axum::extract::State;
use axum::response::IntoResponse;
use bfte_node_ui::ConsensusHistoryEntry;
use maud::html;
use snafu::ResultExt as _;

use crate::ArcUiState;
use crate::error::{OtherSnafu, RequestResult};
use crate::misc::Maud;
use crate::page::NavbarSelector;

pub async fn get(state: State<ArcUiState>) -> RequestResult<impl IntoResponse> {
    let history = state
        .node_api
        .get_consensus_history(1000)
        .await
        .context(OtherSnafu)?;

    let content = render_explorer_page(&history);
    Ok(Maud(
        state
            .render_html_page(
                Some(NavbarSelector::Explorer),
                "Consensus Explorer",
                content,
            )
            .await,
    ))
}

fn render_explorer_page(history: &[ConsensusHistoryEntry]) -> maud::PreEscaped<String> {
    html! {
        div {
            h2 { "Consensus Explorer" }
            p { "Showing the last " (history.len()) " consensus rounds" }

            section {
                h3 { "Consensus History" }
                @if history.is_empty() {
                    p { "No consensus history available." }
                } @else {
                    table {
                        thead {
                            tr {
                                th { "Round" }
                                th { "Timestamp" }
                                th { "Payload Size" }
                                th { "Signatures" }
                            }
                        }
                        tbody {
                            @for entry in history {
                                tr {
                                    td {
                                        @if let Some(ref header) = entry.block_header {
                                            em
                                                data-tooltip=(header.hash())
                                                data-placement="right"
                                                {
                                                (format!("{}", entry.round.to_number()))
                                            }
                                        } @else {
                                            (format!("{}", entry.round.to_number()))
                                        }
                                    }
                                    td {
                                        @if let Some(ref header) = entry.block_header {
                                            @if let Some(datetime) = header.timestamp.to_datetime() {
                                                (datetime.format(&time::format_description::well_known::Rfc3339).unwrap_or_else(|_| "Invalid".to_string()))
                                            } @else {
                                                "Invalid timestamp"
                                            }
                                        } @else {
                                            ""
                                        }
                                    }
                                    td {
                                        @if let Some(ref header) = entry.block_header {
                                            @if !header.is_dummy() {
                                                (format!("{} bytes", header.payload_len.to_number()))
                                            } @else {
                                                ""
                                            }
                                        } @else {
                                            ""
                                        }
                                    }
                                    td {
                                        @if !entry.signatory_peers.is_empty() {
                                            @let signatories_str = entry.signatory_peers.iter()
                                                .map(|peer| format!("{}", peer.to_short()))
                                                .collect::<Vec<_>>()
                                                .join(", ");
                                            em
                                                data-tooltip=(signatories_str)
                                                data-placement="left"
                                            {
                                                (entry.signatory_peers.len())
                                            }
                                        } @else {
                                            ""
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
