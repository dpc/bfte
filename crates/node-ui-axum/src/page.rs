use maud::{DOCTYPE, Markup, html};

use crate::UiState;
use crate::assets::{ROUTE_DATASTAR_JS, ROUTE_LOGO_PNG, ROUTE_PICO_CSS, ROUTE_STYLE_CSS};

impl UiState {
    pub(crate) fn render_html_head(&self, page_title: &str) -> Markup {
        html! {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta name="color-scheme" content="light dark";
                link rel="stylesheet" type="text/css" href=(ROUTE_PICO_CSS);
                link rel="stylesheet" type="text/css" href=(ROUTE_STYLE_CSS);
                link rel="icon" type="image/png" href=(ROUTE_LOGO_PNG);
                title { (page_title) }
                // Load htmx right away so it's immediately available, use defer to make it
                // non-blocking
                script defer type="module" src=(ROUTE_DATASTAR_JS) {}
                // script defer src=(ROUTE_DATASTAR_JS) {}
            }
        }
    }

    pub fn render_html_page(
        &self,
        active_nabvar: NavbarSelector,
        title: &str,
        main_content: Markup,
    ) -> Markup {
        html! {
            (DOCTYPE)
            html lang="en" {
                (self.render_html_head(title))

                body {
                    header {
                        (self.render_page_header(active_nabvar))
                    }

                    main {
                        (main_content)
                    }
                }
            }
        }
    }

    fn render_page_header(&self, active_nabvar: NavbarSelector) -> Markup {
        html! {
            aside {
                nav
                    data-signals__ifmissing="{ nav: { openTabs: {
                        consensus: false,
                        another: false,
                    }}}"
                    data-persist="$nav.openTabs.*"
                {
                    h3 { "BFTE" }
                    div {
                        details
                            data-attr="{ open: $nav.openTabs.consensus }"
                        {
                            summary
                                aria-current=[active_nabvar.is_consensus().then_some("true")]
                                data-on-click__prevent="$nav.openTabs.consensus = !$nav.openTabs.consensus"
                            {
                                "Consensus"
                            }
                            ul {
                                li {
                                    a ."secondary"
                                        data-discover="true"
                                        href="/ui/tdb"
                                        aria-current=[active_nabvar.is_consensus().then_some("page")]
                                    {
                                        "Consensus Status"
                                    }
                                }
                                li {
                                    a ."secondary" data-discover="true" href="/ui/tdb" { "Test 2" }
                                }
                            }
                        }

                        details
                            data-attr="{ open: $nav.openTabs.another }"
                        {
                            summary
                                data-on-click__prevent="$nav.openTabs.another = !$nav.openTabs.another"
                            {
                                "Another"
                            }
                            ul {
                                li {
                                    a ."secondary" data-discover="true" href="/ui/tdb" { "Test 1" }
                                }
                                li {
                                    a ."secondary" data-discover="true" href="/ui/tdb" { "Test 2" }
                                }
                            }
                        }
                    }
                }
            }

        }
    }
}

#[derive(Copy, Clone)]
pub(crate) enum NavbarSelector {
    Consensus,
}

impl NavbarSelector {
    fn is_consensus(self) -> bool {
        matches!(self, NavbarSelector::Consensus)
    }
}
