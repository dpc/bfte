use bfte_consensus_core::module::ModuleId;
use bfte_db::Database;
use bfte_db::ctx::WriteTransactionCtx;
use redb_bincode::redb::{TableError, TableHandle as _};
use redb_bincode::{ReadOnlyTable, ReadTransaction, Table, TableDefinition};

/// A wrapper around [`Database`] that encapsulates module's tables
///
/// This is done by prefixing all table names with `module_{module_id}_`
pub struct ModuleDb {
    module_id: ModuleId,
    inner: Database,
}

impl ModuleDb {
    fn new(module_id: ModuleId, db: Database) -> Self {
        Self {
            module_id,
            inner: db,
        }
    }
}

pub struct ModuleWriteTransaction {
    module_id: ModuleId,
    inner: WriteTransactionCtx,
}

impl ModuleWriteTransaction {
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

pub struct ModuleReadTransaction {
    module_id: ModuleId,
    inner: ReadTransaction,
}

impl ModuleReadTransaction {
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
