use axum::Router;
use axum::routing::{get, post};

use crate::{
    ArcUiState, ROUTE_DS_CURRENT_ROUND, ROUTE_EXPLORER, ROUTE_INIT_CONSENSUS, ROUTE_INVITE, ROUTE_LOGIN,
    ROUTE_MODULE, ROUTE_MODULE_ADD_MODULE_VOTE, ROUTE_MODULE_ADD_PEER_VOTE, ROUTE_MODULE_META_KEY,
    ROUTE_MODULE_META_KEY_REDIRECT, ROUTE_MODULE_META_VOTE, ROUTE_MODULE_REMOVE_PEER_VOTE, ROUTE_UI,
};

pub(crate) mod consensus_status;
pub(crate) mod explorer;
pub(crate) mod init;
pub(crate) mod invite;
pub(crate) mod login;
pub(crate) mod module;

pub(crate) fn make_router() -> Router<ArcUiState> {
    Router::new()
        .route("/", get(consensus_status::root))
        .route(ROUTE_UI, get(consensus_status::get))
        .route(ROUTE_EXPLORER, get(explorer::get))
        .route(ROUTE_LOGIN, get(login::get).post(login::post))
        .route(ROUTE_MODULE, get(module::get))
        .route(ROUTE_MODULE_ADD_PEER_VOTE, post(module::post_add_peer_vote))
        .route(
            ROUTE_MODULE_REMOVE_PEER_VOTE,
            post(module::post_remove_peer_vote),
        )
        .route(
            ROUTE_MODULE_ADD_MODULE_VOTE,
            post(module::post_add_module_vote),
        )
        .route(ROUTE_MODULE_META_KEY, get(module::get_meta_key))
        .route(ROUTE_MODULE_META_KEY_REDIRECT, get(module::get_meta_key_redirect))
        .route(ROUTE_MODULE_META_VOTE, post(module::post_meta_vote))
        .route(ROUTE_INIT_CONSENSUS, get(init::get).post(init::post))
        .route(ROUTE_INVITE, get(invite::get))
        .route(ROUTE_DS_CURRENT_ROUND, get(consensus_status::updates))
}
