#![allow(dead_code)]

use axum::Router;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

// Asset route constants
// pub const BOOTSTRAP_CSS_ROUTE: &str = "/assets/bootstrap.min.css";
pub const ROUTE_DATASTAR_JS: &str = "/assets/datastar/datastar-1-0-0-beta-11-451cf4728ff6863d.js";
pub const ROUTE_DATASTAR_JS_MAP: &str =
    "/assets/datastar/datastar-1-0-0-beta-11-451cf4728ff6863d.js.map";
pub const ROUTE_PICO_CSS: &str = "/assets/pico@2.indigo.min.css";
pub const ROUTE_STYLE_CSS: &str = "/assets/style.css";
pub const ROUTE_LOGO_PNG: &str = "/assets/logo.png";

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
                    "../assets/datastar/datastar-1-0-0-beta-11-451cf4728ff6863d.js"
                )))
            }),
        )
        .route(
            ROUTE_DATASTAR_JS_MAP,
            get(|| async move {
                get_static_json(include_str!(concat!(
                    "../assets/datastar/datastar-1-0-0-beta-11-451cf4728ff6863d.js.map"
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
        // TODO: add some logo
        // .route(
        //     ROUTE_LOGO_PNG,
        //     get(|| async move {
        // get_static_png(include_bytes!("../assets/logo.png")) }),
    }
}
