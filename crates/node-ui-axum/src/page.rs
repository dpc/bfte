use bfte_consensus_core::module::ModuleId;
use maud::{DOCTYPE, Markup, html};

use crate::assets::{
    ROUTE_ALPINEAJAX_JS, ROUTE_ALPINEJS_JS, ROUTE_DATASTAR_JS, ROUTE_LOGO_SVG, ROUTE_PICO_CSS,
    ROUTE_STYLE_CSS,
};
use crate::{ROUTE_EXPLORER, ROUTE_INVITE, ROUTE_UI, UiState};

impl UiState {
    pub(crate) fn render_html_head(&self, page_title: &str) -> Markup {
        html! {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta name="color-scheme" content="light dark";
                title { (page_title) }
                link rel="stylesheet" type="text/css" href=(ROUTE_PICO_CSS);
                link rel="stylesheet" type="text/css" href=(ROUTE_STYLE_CSS);
                link rel="icon" type="image/svg+xml" href=(ROUTE_LOGO_SVG);
                script defer type="module" src=(ROUTE_DATASTAR_JS) {}
                script defer src=(ROUTE_ALPINEAJAX_JS) {}
                script defer src=(ROUTE_ALPINEJS_JS) {}
            }
        }
    }

    pub async fn render_html_page(
        &self,
        active_navbar: Option<NavbarSelector>,
        title: &str,
        main_content: Markup,
    ) -> Markup {
        html! {
            (DOCTYPE)
            html lang="en" {
                (self.render_html_head(title))

                body {
                    header {
                        @if let Some(active_navbar) = active_navbar {
                            (self.render_page_navbar(active_navbar).await)
                        }
                    }

                    main ."container" {
                        div ."main-content" {
                            (main_content)
                        }
                    }
                }
            }
        }
    }

    async fn render_page_navbar(&self, active_nabvar: NavbarSelector) -> Markup {
        html! {
            aside {
                nav
                    data-signals__ifmissing="{ nav: { openTabs: {
                        general: true,
                        modules: false,
                    }}}"
                    data-persist="$nav.openTabs.*"
                {
                    h1 {
                        a
                            href="https://app.radicle.xyz/nodes/radicle.dpc.pw/rad:zii8qFzZhN3vigh8BuxGCuEEp6z4"
                            target="_blank"
                        { "BFTE" }
                    }
                    div {
                        details
                            data-attr="{ open: $nav.openTabs.general}"
                        {
                            summary
                                aria-current=[active_nabvar.is_general().then_some("true")]
                                data-on-click__prevent="$nav.openTabs.general = !$nav.openTabs.general"
                            {
                                "Consensus"
                            }
                            ul {
                                li {
                                    a ."secondary"
                                        data-discover="true"
                                        href=(ROUTE_UI)
                                        aria-current=[active_nabvar.is_general().then_some("page")]
                                    {
                                        "Overview"
                                    }
                                }
                                li {
                                    a ."secondary" data-discover="true" href=(ROUTE_INVITE) { "Invite Code" }
                                }
                                li {
                                    a ."secondary"
                                        data-discover="true"
                                        href=(ROUTE_EXPLORER)
                                        aria-current=[active_nabvar.is_explorer().then_some("page")]
                                    { "Explorer" }
                                }
                            }
                        }

                        details
                            data-attr="{ open: $nav.openTabs.modules }"
                        {
                            summary
                                data-on-click__prevent="$nav.openTabs.modules = !$nav.openTabs.modules"
                                aria-current=[active_nabvar.is_module().then_some("true")]
                            {
                                "Modules"
                            }
                            ul {
                                @for (module_id, kind) in self.modules.get_modules_kinds().await {
                                    @let name = self.modules_inits.get(&kind).expect("Missing module init for the module?!").display_name();
                                    li {
                                        a ."secondary"
                                        data-discover="true"
                                        aria-current=[active_nabvar.is_module_id(module_id).then_some("true")]
                                        href=(format!("/ui/module/{module_id}")) { (format!("{module_id}. {name}")) }
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

#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum NavbarSelector {
    General,
    Explorer,
    Module(ModuleId),
}

impl NavbarSelector {
    fn is_general(self) -> bool {
        matches!(self, NavbarSelector::General)
    }
    fn is_explorer(self) -> bool {
        matches!(self, NavbarSelector::Explorer)
    }
    fn is_module(self) -> bool {
        matches!(self, NavbarSelector::Module(_))
    }
    fn is_module_id(self, module_id: ModuleId) -> bool {
        self == NavbarSelector::Module(module_id)
    }
}
