use std::sync::Arc;

use bfte::Bfte;
use bfte_module_meta::MetaModuleInit;
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
    Bfte::builder()
        .with_module_init(Arc::new(MetaModuleInit::new()))
        .run()
        .await?;
    Ok(())
}
