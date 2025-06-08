use axum::Extension;
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Redirect, Response};
use serde::{Deserialize, Serialize};
use snafu::ResultExt as _;
use tower_sessions::Session;

use crate::error::{InternalServerSnafu, LoginRequiredSnafu, OtherSnafu, RequestError};
use crate::{ArcUiState, ROUTE_INIT_CONSENSUS, ROUTE_LOGIN, ROUTE_UI};

pub async fn cache_control(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;

    if let Some(content_type) = response.headers().get(CONTENT_TYPE) {
        const NON_CACHEABLE_CONTENT_TYPES: &[&str] = &["text/html"];
        const SHORT_CACHE_CONTENT_TYPES: &[&str] = &["text/css"];

        let cache_duration_secs = if SHORT_CACHE_CONTENT_TYPES
            .iter()
            .any(|&ct| content_type.as_bytes().starts_with(ct.as_bytes()))
        {
            Some(10 * 60)
        } else if NON_CACHEABLE_CONTENT_TYPES
            .iter()
            .any(|&ct| content_type.as_bytes().starts_with(ct.as_bytes()))
        {
            None
        } else {
            Some(60 * 60)
        };

        if let Some(dur) = cache_duration_secs {
            let value = format!("public, max-age={}", dur);

            response.headers_mut().insert(
                CACHE_CONTROL,
                HeaderValue::from_str(&value).expect("Can't fail"),
            );
        }
    }

    response
}

pub const SESSION_KEY: &str = "bfte_session";
#[derive(Clone, Deserialize, Serialize)]
pub struct UserAuth;

// Check if the request is for /ui/login or requires authentication
pub(crate) async fn require_auth(
    session: Session,
    req: Request,
    next: Next,
) -> Result<Response, RequestError> {
    if req.uri().path() == ROUTE_LOGIN {
        return Ok(next.run(req).await);
    }

    let user_result: Result<Option<UserAuth>, _> = session.get(SESSION_KEY).await.map_err(|_| {
        InternalServerSnafu {
            msg: "session store error",
        }
        .build()
    });

    match user_result {
        Ok(Some(_user)) => {
            // User is authenticated, proceed to the next handler
            Ok(next.run(req).await)
        }
        Ok(None) => {
            // No user session, return login required error
            Err((LoginRequiredSnafu {
                path: Some(req.uri().path().to_owned()),
            })
            .build())
        }
        Err(e) => Err(e),
    }
}

/// Redirect from/to consensus initialization page if needed
pub(crate) async fn consensus_init(
    Extension(state): Extension<ArcUiState>,
    req: Request,
    next: Next,
) -> Result<Response, RequestError> {
    Ok(
        if state
            .node_api
            .is_consensus_initialized()
            .context(OtherSnafu)?
        {
            if req.uri().path() == ROUTE_INIT_CONSENSUS {
                Redirect::to(ROUTE_UI).into_response()
            } else {
                next.run(req).await
            }
        } else {
            if req.uri().path() == ROUTE_INIT_CONSENSUS || req.uri().path() == ROUTE_LOGIN {
                next.run(req).await
            } else {
                Redirect::to(ROUTE_INIT_CONSENSUS).into_response()
            }
        },
    )
}
