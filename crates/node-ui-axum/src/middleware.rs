use axum::Extension;
use axum::extract::Request;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, REFERER};
use axum::http::{HeaderValue, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Redirect, Response};
use maud::html;
use serde::{Deserialize, Serialize};
use snafu::ResultExt as _;
use tower_sessions::Session;

use crate::error::{InternalServerSnafu, LoginRequiredSnafu, OtherSnafu, RequestError};
use crate::misc::Maud;
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
                path: if req.method() == Method::GET {
                    // Only redirect after login if the call was GET, otherwise things get weird
                    Some(req.uri().path().to_owned())
                } else {
                    // For other calls, redirect to the referer, if available
                    req.headers()
                        .get(REFERER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value_s| {
                            url::Url::parse(value_s)
                                .ok()
                                .map(|url| url.path().to_string())
                        })
                },
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

/// Turn non-HTML error responses into proper, hypermedia ones
pub(crate) async fn hypermedia_errors(req: Request, next: Next) -> Result<Response, RequestError> {
    let resp = next.run(req).await;

    let status = resp.status();
    if !(status.is_client_error() || resp.status().is_server_error())
        || resp
            .headers()
            .get(CONTENT_TYPE)
            .is_none_or(|ct| ct.as_bytes().starts_with(b"text/html"))
    {
        return Ok(resp);
    }

    let (parts, _body) = resp.into_parts();

    let msg = if status.is_client_error() {
        "Invalid Data"
    } else {
        "Server Error"
    };
    let (mut new_parts, new_body) = (Maud(html! {
        p id="error-response" { (msg) }
    }))
    .into_response()
    .into_parts();

    new_parts.status = parts.status;
    Ok(Response::from_parts(new_parts, new_body))
}
