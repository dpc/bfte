use bfte_db::ctx::WriteTransactionCtx;
use bfte_db::error::DbResult;

use crate::{NodeApp, tables};

impl NodeApp {
    pub(super) fn init_tables_dbtx(tx: &WriteTransactionCtx) -> DbResult<()> {
        tx.open_table(&tables::app_cur_round::TABLE)?;
        tx.open_table(&tables::app_cur_peer_set::TABLE)?;
        Ok(())
    }
}
