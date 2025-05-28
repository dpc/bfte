use std::io;

use bfte_util_error::{Whatever, WhateverResult};
use snafu::FromString as _;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

pub fn init_logging() -> WhateverResult<()> {
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .try_init()
        .map_err(|_| Whatever::without_source("Failed to initialize logging".to_string()))?;

    Ok(())
}
