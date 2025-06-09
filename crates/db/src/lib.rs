pub mod ctx;
pub mod error;

use std::path::PathBuf;
use std::sync::Arc;

use bfte_util_error::fmt::FmtCompact as _;
use ctx::WriteTransactionCtx;
use error::{
    CommitSnafu, DatabaseSnafu, DbResult, DbTxError, DbTxResult, InvalidPathSnafu, JoinSnafu,
    TransactionSnafu,
};
use redb_bincode::{ReadTransaction, redb};
use snafu::{OptionExt as _, ResultExt as _};
use tracing::{debug, instrument, warn};

const LOG_TARGET: &str = "bfte::consensus::db";

#[derive(Debug)]
pub struct Database {
    inner: redb_bincode::Database,
    commit_hook_order_lock: Arc<std::sync::Mutex<()>>,
    ephemeral: bool,
}

impl Database {
    pub async fn new_in_memory() -> DbResult<Database> {
        debug!(target: LOG_TARGET, "Opening in-memory database");
        let inner = redb::Database::builder()
            .create_with_backend(redb::backends::InMemoryBackend::new())
            .context(DatabaseSnafu)?;
        Self::open_inner(inner, true).await
    }

    pub async fn open(path: impl Into<PathBuf>) -> DbResult<Database> {
        let path = path.into();
        tokio::fs::create_dir_all(path.parent().context(InvalidPathSnafu)?).await?;
        debug!(target: LOG_TARGET, path = %path.display(), "Opening database…");

        let inner = tokio::task::spawn_blocking(move || {
            let mut db = redb::Database::create(path)?;
            let _ = db.compact().inspect_err(|err| {
                warn!(target: LOG_TARGET, err = %err.fmt_compact(), "Failed to compact database");
            });
            Ok(db)
        })
        .await
        .context(JoinSnafu)?
        .context(DatabaseSnafu)?;

        Self::open_inner(inner, false).await
    }

    #[instrument(skip_all)]
    async fn open_inner(inner: redb::Database, ephemeral: bool) -> DbResult<Database> {
        let inner = redb_bincode::Database::from(inner);
        let commit_hook_order_lock = Arc::new(std::sync::Mutex::new(()));

        let s = Self {
            inner,
            commit_hook_order_lock,
            ephemeral,
        };

        Ok(s)
    }

    async fn read_with_inner_falliable<T, E>(
        inner: &redb_bincode::Database,
        f: impl FnOnce(&'_ ReadTransaction) -> DbTxResult<T, E>,
    ) -> DbTxResult<T, E>
    where
        E: snafu::Error + 'static,
    {
        tokio::task::block_in_place(|| {
            let dbtx = inner.begin_read().context(TransactionSnafu)?;
            let res = f(&dbtx)?;

            Ok(res)
        })
    }

    async fn write_with_inner_falliable<T, E>(
        inner: &redb_bincode::Database,
        commit_hook_order_lock: Arc<std::sync::Mutex<()>>,
        f: impl FnOnce(&'_ WriteTransactionCtx) -> DbTxResult<T, E>,
    ) -> DbTxResult<T, E>
    where
        E: snafu::Error + 'static,
    {
        tokio::task::block_in_place(|| {
            let mut dbtx = WriteTransactionCtx::new(
                inner.begin_write().context(TransactionSnafu)?,
                commit_hook_order_lock,
            );
            let res = f(&mut dbtx)?;
            dbtx.commit().context(CommitSnafu)?;

            Ok(res)
        })
    }

    async fn write_with_inner<T>(
        inner: &redb_bincode::Database,
        commit_hook_order_lock: Arc<std::sync::Mutex<()>>,
        f: impl FnOnce(&'_ WriteTransactionCtx) -> DbResult<T>,
    ) -> DbResult<T> {
        tokio::task::block_in_place(|| {
            let mut dbtx = WriteTransactionCtx::new(
                inner.begin_write().context(TransactionSnafu)?,
                commit_hook_order_lock,
            );
            let res = f(&mut dbtx)?;

            dbtx.commit().context(CommitSnafu)?;

            Ok(res)
        })
    }

    async fn read_with_inner<T>(
        inner: &redb_bincode::Database,
        f: impl FnOnce(&'_ ReadTransaction) -> DbResult<T>,
    ) -> DbResult<T> {
        tokio::task::block_in_place(|| {
            let mut dbtx = inner.begin_read().context(TransactionSnafu)?;

            f(&mut dbtx)
        })
    }

    pub async fn write_with<T>(
        &self,
        f: impl FnOnce(&'_ WriteTransactionCtx) -> DbResult<T>,
    ) -> DbResult<T> {
        Self::write_with_inner(&self.inner, self.commit_hook_order_lock.clone(), f).await
    }

    pub async fn write_with_expect_falliable<T, E>(
        &self,
        f: impl FnOnce(&'_ WriteTransactionCtx) -> DbTxResult<T, E>,
    ) -> Result<T, E>
    where
        E: snafu::Error + 'static,
    {
        match Self::write_with_inner_falliable(&self.inner, self.commit_hook_order_lock.clone(), f)
            .await
        {
            Ok(o) => Ok(o),
            Err(DbTxError::DbError { source, location }) => {
                panic!("Database error: {source:#} at {location}")
            }
            Err(DbTxError::TxError {
                source,
                location: _,
            }) => Err(source),
        }
    }

    /// Do a writeable database transaction and panic on internal db errors
    ///
    /// If the handler `f` can fail for logical reasons, use
    /// [`Self::write_with_expect_falliable`]
    pub async fn write_with_expect<T>(
        &self,
        f: impl FnOnce(&'_ WriteTransactionCtx) -> DbResult<T>,
    ) -> T {
        Self::write_with_inner(&self.inner, self.commit_hook_order_lock.clone(), f)
            .await
            .expect("Fatal database error")
    }

    pub async fn read_with<T>(
        &self,
        f: impl FnOnce(&'_ ReadTransaction) -> DbResult<T>,
    ) -> DbResult<T> {
        Self::read_with_inner(&self.inner, f).await
    }

    pub async fn read_with_expect_falliable<T, E>(
        &self,
        f: impl FnOnce(&'_ ReadTransaction) -> DbTxResult<T, E>,
    ) -> Result<T, E>
    where
        E: snafu::Error + 'static,
    {
        match Self::read_with_inner_falliable(&self.inner, f).await {
            Ok(o) => Ok(o),
            Err(DbTxError::DbError { source, location }) => {
                panic!("Database error: {source:#} at {location}")
            }
            Err(DbTxError::TxError {
                source,
                location: _,
            }) => Err(source),
        }
    }

    pub async fn read_with_expect<T>(
        &self,
        f: impl FnOnce(&'_ ReadTransaction) -> DbResult<T>,
    ) -> T {
        Self::read_with_inner(&self.inner, f)
            .await
            .expect("Fatal database error")
    }

    pub fn is_ephemeral(&self) -> bool {
        self.ephemeral
    }
}
