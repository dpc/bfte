use axum::extract::FromRequest;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use maud::Markup;

use crate::error::RequestError;

#[derive(Clone, Debug)]
#[must_use]
pub struct Html(pub String);

impl IntoResponse for Html {
    fn into_response(self) -> Response {
        (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            )],
            self.0,
        )
            .into_response()
    }
}

#[derive(Clone, Debug)]
#[must_use]
pub struct Maud(pub Markup);

impl IntoResponse for Maud {
    fn into_response(self) -> Response {
        (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            )],
            self.0.0,
        )
            .into_response()
    }
}

#[derive(FromRequest)]
/// Error by the user
#[from_request(via(axum::Json), rejection(RequestError))]
pub struct AppJson<T>(pub T);

impl<T> IntoResponse for AppJson<T>
where
    axum::Json<T>: IntoResponse,
{
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}
