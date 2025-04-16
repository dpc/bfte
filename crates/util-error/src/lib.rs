// SPDX-License-Identifier: MIT

use snafu::Snafu;

pub mod fmt;

pub type BoxedError = Box<dyn std::error::Error + Send + Sync + 'static>;
pub type BoxedErrorResult<T> = std::result::Result<T, BoxedError>;
pub type WhateverResult<T> = std::result::Result<T, Whatever>;

/// Snafu's `Whatever`, but `Send + Sync`
#[derive(Debug, Snafu)]
#[snafu(whatever, display("{message}"))]
pub struct Whatever {
    #[snafu(source(from(Box<dyn std::error::Error + Send + Sync>, Some)))]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,

    message: String,
}
