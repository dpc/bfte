use bfte::Bfte;
use bfte_util_error::{BoxedError, WhateverResult};
use snafu::Snafu;

#[derive(Debug, Snafu)]
#[snafu(transparent)]
struct CliError {
    source: BoxedError,
}

#[tokio::main]
#[snafu::report]
async fn main() -> WhateverResult<()> {
    Bfte::builder().run().await?;
    Ok(())
}
