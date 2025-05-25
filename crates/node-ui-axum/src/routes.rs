use axum::Router;
use axum::routing::get;

use crate::{ArcUiState, ROUTE_DS_CURRENT_ROUND, ROUTE_LOGIN, ROUTE_UI};

pub(crate) mod login;
pub(crate) mod root;

pub(crate) fn make_router() -> Router<ArcUiState> {
    Router::new()
        .route("/", get(root::root))
        .route(ROUTE_UI, get(root::get))
        .route(ROUTE_LOGIN, get(login::get).post(login::post))
        .route(ROUTE_DS_CURRENT_ROUND, get(root::current_round))
}
