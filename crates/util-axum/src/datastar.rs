use std::convert::Infallible;
use std::ops;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct DatastarRequest(bool);

impl From<DatastarRequest> for bool {
    fn from(val: DatastarRequest) -> Self {
        val.0
    }
}

impl ops::Deref for DatastarRequest {
    type Target = bool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> FromRequestParts<S> for DatastarRequest
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let Some(header_value) = parts.headers.get("datastar-request") else {
            return Ok(Self(false));
        };

        Ok(Self(header_value == "true"))
    }
}
