// TODO:
#![allow(dead_code)]

use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use bfte_util_error::Whatever;
use maud::html;
use serde::Serialize;
use snafu::Snafu;

use crate::ROUTE_LOGIN;
use crate::misc::{AppJson, Maud};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum UserRequestError {
    SomethingNotFound,
    InvalidData,
    #[snafu(display("Wrong Password"))]
    WrongPassword,
    Other {
        source: Whatever,
    },
}

impl IntoResponse for &UserRequestError {
    fn into_response(self) -> Response {
        let html = Maud(html! {
            p id="error-response" { (self.to_string()) }
        });

        let (status_code, html) = match self {
            UserRequestError::SomethingNotFound => (StatusCode::NOT_FOUND, html),
            UserRequestError::InvalidData => (StatusCode::BAD_REQUEST, html),
            UserRequestError::WrongPassword => (StatusCode::BAD_REQUEST, html),
            UserRequestError::Other { .. } => (StatusCode::BAD_REQUEST, html),
        };
        (status_code, html).into_response()
    }
}

// How we want user errors responses to be serialized
#[derive(Serialize)]
pub struct UserErrorResponse {
    pub message: String,
}

#[derive(Debug, Snafu)]
pub enum RequestError {
    #[snafu(visibility(pub(crate)))]
    InternalServerError { msg: &'static str },
    #[snafu(visibility(pub(crate)))]
    ShuttingDown { source: Whatever },

    #[snafu(visibility(pub(crate)))]
    LoginRequired {
        /// Path to redirect to after successful login
        path: Option<String>,
    },

    #[snafu(transparent)]
    User { source: UserRequestError },
}
pub type RequestResult<T> = std::result::Result<T, RequestError>;

impl IntoResponse for RequestError {
    fn into_response(self) -> Response {
        let (status_code, message) = match root_cause(&self).downcast_ref::<UserRequestError>() {
            Some(user_err) => {
                return user_err.into_response();
            }
            _ => match self {
                RequestError::LoginRequired { path } => {
                    // let headers = [
                    //     (
                    //         HeaderName::from_static("hx-redirect"),
                    //         HeaderValue::from_static(ROUTE_LOGIN),
                    //     ),
                    //     (LOCATION, HeaderValue::from_static(ROUTE_LOGIN)),
                    // ];
                    // return (StatusCode::SEE_OTHER, headers).into_response();

                    return Redirect::to(&login_redirect_path(path)).into_response();
                }
                RequestError::User { source } => {
                    return source.into_response();
                }
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal Service Error".to_owned(),
                ),
            },
        };

        (status_code, AppJson(UserErrorResponse { message })).into_response()
    }
}

pub(crate) fn login_redirect_path(redirect: Option<String>) -> String {
    if let Some(path) = redirect {
        format!("{}?redirect={}", ROUTE_LOGIN, &urlencoding::encode(&path))
    } else {
        ROUTE_LOGIN.to_string()
    }
}

fn root_cause<E>(e: &E) -> &(dyn std::error::Error + 'static)
where
    E: std::error::Error + 'static,
{
    let mut cur_source: &dyn std::error::Error = e;

    while let Some(new_source) = cur_source.source() {
        cur_source = new_source;
    }
    cur_source
}
