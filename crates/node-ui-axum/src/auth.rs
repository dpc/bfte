use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;

use crate::UiState;
use crate::error::{InternalServerSnafu, LoginRequiredSnafu, RequestError};

#[derive(Clone, Deserialize, Serialize)]
pub struct UserAuth;

impl UserAuth {
    pub(crate) fn new() -> Self {
        Self
    }
}

pub const SESSION_KEY: &str = "bfte_session";

impl FromRequestParts<Arc<UiState>> for UserAuth {
    type Rejection = RequestError;

    async fn from_request_parts(
        req: &mut request::Parts,
        state: &Arc<UiState>,
    ) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(req, state)
            .await
            .map_err(|(_, msg)| InternalServerSnafu { msg }.build())?;

        // Try to get the user session from the session store
        let user_result: Result<Option<UserAuth>, _> =
            session.get(SESSION_KEY).await.map_err(|_| {
                InternalServerSnafu {
                    msg: "session store error",
                }
                .build()
            });

        match user_result {
            Ok(Some(user)) => Ok(user),
            Ok(None) => Err(LoginRequiredSnafu.build()),
            Err(e) => Err(e),
        }
    }
}
