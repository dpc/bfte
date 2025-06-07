use std::sync::Arc;

use bfte_consensus_core::module::ModuleId;
use bfte_db::Database;
use bfte_db::ctx::WriteTransactionCtx;
pub use bfte_db::error::{DbError, DbResult, DbTxResult};
use redb_bincode::redb::{TableError, TableHandle as _};
use redb_bincode::{ReadOnlyTable, ReadTransaction, ReadableTable, Table, TableDefinition};

/// A wrapper around [`Database`] that encapsulates module's tables
///
/// This is done by prefixing all table names with `module_{module_id}_`
pub struct ModuleDatabase {
    module_id: ModuleId,
    inner: Arc<Database>,
}

impl ModuleDatabase {
    pub fn new(module_id: ModuleId, db: Arc<Database>) -> Self {
        Self {
            module_id,
            inner: db,
        }
    }
}

impl ModuleDatabase {
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
    inner: &'a WriteTransactionCtx,
}

impl<'s> ModuleWriteTransactionCtx<'s> {
    pub fn new(module_id: ModuleId, inner: &'s WriteTransactionCtx) -> Self {
        Self { module_id, inner }
    }
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

    pub fn on_commit(&self, f: impl FnOnce() + 'static) {
        self.inner.on_commit(f);
    }
}

pub struct ModuleReadTransaction<'a> {
    module_id: ModuleId,
    inner: &'a ReadTransaction,
}

impl<'s> ModuleReadTransaction<'s> {
    pub fn new(module_id: ModuleId, inner: &'s ReadTransaction) -> Self {
        Self { module_id, inner }
    }
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

pub trait ModuleReadableTransaction<'s> {
    type Table<K, V>: ReadableTable<K, V>
    where
        K: bincode::Encode + bincode::Decode<()>,
        V: bincode::Encode + bincode::Decode<()>;

    fn open_table<K, V>(
        &self,
        table_def: &TableDefinition<'_, K, V>,
    ) -> Result<Self::Table<K, V>, TableError>
    where
        K: bincode::Encode + bincode::Decode<()>,
        V: bincode::Encode + bincode::Decode<()>;
}

impl<'s> ModuleReadableTransaction<'s> for ModuleReadTransaction<'s> {
    type Table<K, V>
        = ReadOnlyTable<K, V>
    where
        K: bincode::Encode + bincode::Decode<()>,
        V: bincode::Encode + bincode::Decode<()>;
    fn open_table<K, V>(
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

impl<'s> ModuleReadableTransaction<'s> for ModuleWriteTransactionCtx<'s> {
    type Table<K, V>
        = Table<'s, K, V>
    where
        K: bincode::Encode + bincode::Decode<()>,
        V: bincode::Encode + bincode::Decode<()>;

    fn open_table<K, V>(
        &self,
        table_def: &TableDefinition<'_, K, V>,
    ) -> Result<Table<'s, K, V>, TableError>
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
