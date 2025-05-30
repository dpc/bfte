use std::sync::Arc;

use bfte_consensus_core::module::ModuleId;
use bfte_db::Database;
use bfte_db::ctx::WriteTransactionCtx;
use bfte_db::error::{DbResult, DbTxResult};
use redb_bincode::redb::{TableError, TableHandle as _};
use redb_bincode::{ReadOnlyTable, ReadTransaction, Table, TableDefinition};

/// A wrapper around [`Database`] that encapsulates module's tables
///
/// This is done by prefixing all table names with `module_{module_id}_`
pub struct ModuleDb {
    module_id: ModuleId,
    inner: Arc<Database>,
}

impl ModuleDb {
    pub(crate) fn new(module_id: ModuleId, db: Arc<Database>) -> Self {
        Self {
            module_id,
            inner: db,
        }
    }
}

impl ModuleDb {
    /// See [`Database::write_with`]
    pub async fn write_with<T>(
        &self,
        f: impl FnOnce(&'_ ModuleWriteTransactionCtx) -> DbResult<T>,
    ) -> DbResult<T> {
        self.inner
            .write_with(|ctx| {
                f(&ModuleWriteTransactionCtx {
                    module_id: self.module_id,
                    inner: ctx,
                })
            })
            .await
    }

    /// See [`Database::write_with_expect_falliable`]
    pub async fn write_with_expect_falliable<T, E>(
        &self,
        f: impl FnOnce(&'_ ModuleWriteTransactionCtx) -> DbTxResult<T, E>,
    ) -> Result<T, E>
    where
        E: snafu::Error + 'static,
    {
        self.inner
            .write_with_expect_falliable(|ctx| {
                f(&ModuleWriteTransactionCtx {
                    module_id: self.module_id,
                    inner: ctx,
                })
            })
            .await
    }

    /// See [`Database::write_with_expect`]
    pub async fn write_with_expect<T>(
        &self,
        f: impl FnOnce(&'_ ModuleWriteTransactionCtx) -> DbResult<T>,
    ) -> T {
        self.inner
            .write_with_expect(|ctx| {
                f(&ModuleWriteTransactionCtx {
                    module_id: self.module_id,
                    inner: ctx,
                })
            })
            .await
    }

    /// See [`Database::read_with`]
    pub async fn read_with<T>(
        &self,
        f: impl FnOnce(&'_ ModuleReadTransaction) -> DbResult<T>,
    ) -> DbResult<T> {
        self.inner
            .read_with(|ctx| {
                f(&ModuleReadTransaction {
                    module_id: self.module_id,
                    inner: ctx,
                })
            })
            .await
    }

    /// See [`Database::read_with_expect_falliable`]
    pub async fn read_with_expect_falliable<T, E>(
        &self,
        f: impl FnOnce(&'_ ModuleReadTransaction) -> DbTxResult<T, E>,
    ) -> Result<T, E>
    where
        E: snafu::Error + 'static,
    {
        self.inner
            .read_with_expect_falliable(|ctx| {
                f(&ModuleReadTransaction {
                    module_id: self.module_id,
                    inner: ctx,
                })
            })
            .await
    }

    /// See [`Database::read_with_expect`]
    pub async fn read_with_expect<T>(
        &self,
        f: impl FnOnce(&'_ ModuleReadTransaction) -> DbResult<T>,
    ) -> T {
        self.inner
            .read_with_expect(|ctx| {
                f(&ModuleReadTransaction {
                    module_id: self.module_id,
                    inner: ctx,
                })
            })
            .await
    }
}

pub struct ModuleWriteTransactionCtx<'a> {
    module_id: ModuleId,
    inner: &'a WriteTransactionCtx, /* commit_hook_order_lock: Arc<std::sync::Mutex<()>>,
                                     * dbtx: WriteTransaction,
                                     * on_commit: std::sync::Mutex<Vec<Box<dyn FnOnce() +
                                     * 'static>>>, */
}

impl ModuleWriteTransactionCtx<'_> {
    pub fn open_table<K, V>(
        &self,
        table_def: &TableDefinition<'_, K, V>,
    ) -> Result<Table<K, V>, TableError>
    where
        K: bincode::Encode + bincode::Decode<()>,
        V: bincode::Encode + bincode::Decode<()>,
    {
        self.inner.open_table(&TableDefinition::new(&format!(
            "module_{}_{}",
            self.module_id,
            table_def.as_raw().name()
        )))
    }
}

pub struct ModuleReadTransaction<'a> {
    module_id: ModuleId,
    inner: &'a ReadTransaction,
}

impl ModuleReadTransaction<'_> {
    pub fn open_table<K, V>(
        &self,
        table_def: &TableDefinition<'_, K, V>,
    ) -> Result<ReadOnlyTable<K, V>, TableError>
    where
        K: bincode::Encode + bincode::Decode<()>,
        V: bincode::Encode + bincode::Decode<()>,
    {
        self.inner.open_table(&TableDefinition::new(&format!(
            "module_{}_{}",
            self.module_id,
            table_def.as_raw().name()
        )))
    }
}
