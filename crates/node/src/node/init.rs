use bfte_db::ctx::WriteTransactionCtx;
use bfte_db::error::DbResult;

use super::Node;
use crate::tables;

impl Node {
    pub(super) fn init_tables_tx(tx: &WriteTransactionCtx) -> DbResult<()> {
        tx.open_table(&tables::ui_pass_hash::TABLE)?;
        Ok(())
    }
}
