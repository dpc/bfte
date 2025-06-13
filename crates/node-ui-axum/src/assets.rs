#![allow(dead_code)]

use axum::Router;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

// Asset route constants
// pub const BOOTSTRAP_CSS_ROUTE: &str = "/assets/bootstrap.min.css";
pub const ROUTE_DATASTAR_JS: &str = "/assets/datastar/datastar-1-0-0-rc-11.js";
pub const ROUTE_DATASTAR_JS_MAP: &str = "/assets/datastar/datastar-1-0-0-rc-11.js.map";
pub const ROUTE_ALPINEJS_JS: &str = "/assets/alpine/alpinejs-3.18.8.js";
pub const ROUTE_ALPINEAJAX_JS: &str = "/assets/alpine/alpine-ajax-0.12.2.js";
pub const ROUTE_PICO_CSS: &str = "/assets/pico@2.indigo.min.css";
pub const ROUTE_STYLE_CSS: &str = "/assets/style.css";
pub const ROUTE_LOGO_SVG: &str = "/assets/logo.svg";

pub(crate) fn get_static_asset(content_type: &'static str, body: &'static [u8]) -> Response {
    (
        [(CONTENT_TYPE, content_type)],
        [(CACHE_CONTROL, format!("public, max-age={}", 60 * 60))],
        body,
    )
        .into_response()
}

pub(crate) fn get_static_css(body: &'static str) -> Response {
    get_static_asset("text/css", body.as_bytes())
}

pub(crate) fn get_static_png(body: &'static [u8]) -> Response {
    get_static_asset("image/png", body)
}

pub(crate) fn get_static_js(body: &'static str) -> Response {
    get_static_asset("application/javascript", body.as_bytes())
}

pub(crate) fn get_static_json(body: &'static str) -> Response {
    get_static_asset("application/json", body.as_bytes())
}
pub(crate) fn get_static_svg(body: &'static [u8]) -> Response {
    get_static_asset("image/svg+xml", body)
}

pub(crate) trait WithStaticRoutesExt {
    fn with_static_routes(self) -> Self;
}

impl<S> WithStaticRoutesExt for Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    fn with_static_routes(self) -> Self {
        self.route(
            ROUTE_DATASTAR_JS,
            get(|| async move {
                get_static_js(include_str!(concat!(
                    "../assets/datastar/datastar-1-0-0-rc-11.js"
                )))
            }),
        )
        .route(
            ROUTE_DATASTAR_JS_MAP,
            get(|| async move {
                get_static_json(include_str!(concat!(
                    "../assets/datastar/datastar-1-0-0-rc-11.js.map"
                )))
            }),
        )
        .route(
            ROUTE_ALPINEJS_JS,
            get(|| async move {
                get_static_js(include_str!(concat!("../assets/alpine/alpinejs-3.18.8.js")))
            }),
        )
        .route(
            ROUTE_ALPINEAJAX_JS,
            get(|| async move {
                get_static_js(include_str!(concat!(
                    "../assets/alpine/alpine-ajax-0.12.2.js"
                )))
            }),
        )
        .route(
            ROUTE_STYLE_CSS,
            get(|| async move { get_static_css(include_str!("../assets/style.css")) }),
        )
        .route(
            ROUTE_PICO_CSS,
            get(|| async move { get_static_css(include_str!("../assets/pico@2.indigo.min.css")) }),
        )
        .route(
            ROUTE_LOGO_SVG,
            get(|| async move { get_static_svg(include_bytes!("../assets/logo.svg")) }),
        )
    }
}
