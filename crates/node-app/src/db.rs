use bfte_consensus_core::block::BlockRound;
use bfte_db::error::DbResult;

use crate::NodeApp;
use crate::tables::{self, BlockCItemIdx};

impl NodeApp {
    pub async fn load_cur_round_and_idx(&self) -> (BlockRound, BlockCItemIdx) {
        self.db
            .read_with_expect(|dbtx| {
                let tbl = dbtx.open_table(&tables::app_cur_round::TABLE)?;

                Ok(tbl.get(&())?.map(|v| v.value()).unwrap_or_default())
            })
            .await
    }

    pub(crate) fn save_cur_round_and_idx_dbtx(
        dbtx: &bfte_db::ctx::WriteTransactionCtx,
        cur_round: BlockRound,
        citem_idx: BlockCItemIdx,
    ) -> DbResult<()> {
        let mut tbl = dbtx.open_table(&tables::app_cur_round::TABLE)?;

        let _ = tbl.insert(&(), &(cur_round, citem_idx))?;
        Ok(())
    }
}
