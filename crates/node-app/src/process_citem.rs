use bfte_consensus_core::block::{BlockHeader, BlockRound};
use bfte_consensus_core::citem::CItem;
use bfte_consensus_core::module::ModuleId;
use bfte_db::error::TxSnafu;
use bfte_module::effect::ModuleCItemEffect;
use bfte_module::module::db::{ModuleReadTransaction, ModuleWriteTransactionCtx};
use bfte_util_error::Whatever;
use bfte_util_error::fmt::FmtCompact as _;
use snafu::{OptionExt as _, ResultExt as _, Snafu};
use tracing::debug;

use super::NodeApp;
use crate::LOG_TARGET;
use crate::tables::BlockCItemIdx;

#[derive(Debug, Snafu)]
pub enum ProcessCItemError {
    UnknownModuleId {
        module_id: ModuleId,
    },
    ProcessingCItemFailed {
        source: Whatever,
        module_id: ModuleId,
    },
    ProcessingInputFailed {
        source: Whatever,
        module_id: ModuleId,
    },
    ProcessingOutputFailed {
        source: Whatever,
        module_id: ModuleId,
    },
    ProcessingEffectFailed {
        source: Whatever,
        module_id: ModuleId,
    },
}

pub type ProcessCItemResult<T> = Result<T, ProcessCItemError>;
impl NodeApp {
    pub(crate) async fn process_citem(
        &self,
        (cur_round, cur_citem_idx): (BlockRound, BlockCItemIdx),
        block_header: &BlockHeader,
        citem: &CItem,
    ) {
        if let Err(err) = self
            .process_citem_try((cur_round, cur_citem_idx), block_header.round, citem)
            .await
        {
            debug!(target: LOG_TARGET, err = %err.fmt_compact(), %cur_round, %cur_citem_idx, "Invalid consensus item" );
            // If processing failed, we need to advance the position separately
            self.db
                .write_with_expect(|dbtx| {
                    Self::save_cur_round_and_idx(dbtx, cur_round, cur_citem_idx.next())
                })
                .await;
        }
    }
    pub(crate) async fn process_citem_try(
        &self,
        (cur_round, cur_citem_idx): (BlockRound, BlockCItemIdx),
        block_round: BlockRound,
        citem: &CItem,
    ) -> ProcessCItemResult<()> {
        let modules = self.modules.read().await;

        // First, collect all effects with a read-only transaction
        let effects = self
            .db
            .read_with_expect_falliable(|dbtx| {
                let mut all_effects = Vec::new();

                match citem {
                    CItem::ModuleCItem(module_citem) => {
                        let module_id = module_citem.module_id();
                        let module = modules
                            .get(&module_id)
                            .context(UnknownModuleIdSnafu { module_id })
                            .context(TxSnafu)?;
                        let module_kind = module.config.kind;

                        let module_dbtx = ModuleReadTransaction::new(module_id, dbtx);

                        let effects = module
                            .process_citem(&module_dbtx, block_round, module_citem.inner())
                            .context(ProcessingCItemFailedSnafu { module_id })
                            .context(TxSnafu)?
                            .into_iter()
                            .map(|inner| ModuleCItemEffect::new(module_kind, inner));
                        all_effects.extend(effects);
                    }
                    CItem::Transaction(transaction) => {
                        // Process all inputs
                        for input in &transaction.inner.inputs {
                            let module_id = input.module_id();
                            let module = modules
                                .get(&module_id)
                                .context(UnknownModuleIdSnafu { module_id })
                                .context(TxSnafu)?;
                            let module_kind = module.config.kind;

                            let module_dbtx = ModuleReadTransaction::new(module_id, dbtx);

                            let effects = module
                                .process_input(&module_dbtx, input.inner())
                                .context(ProcessingInputFailedSnafu { module_id })
                                .context(TxSnafu)?
                                .into_iter()
                                .map(|inner| ModuleCItemEffect::new(module_kind, inner));
                            all_effects.extend(effects);
                        }

                        // Process all outputs
                        for output in &transaction.inner.outputs {
                            let module_id = output.module_id();
                            let module = modules
                                .get(&module_id)
                                .context(UnknownModuleIdSnafu { module_id })
                                .context(TxSnafu)?;
                            let module_kind = module.config.kind;

                            let module_dbtx = ModuleReadTransaction::new(module_id, dbtx);

                            let effects = module
                                .process_output(&module_dbtx, output.inner())
                                .context(ProcessingOutputFailedSnafu { module_id })
                                .context(TxSnafu)?
                                .into_iter()
                                .map(|inner| ModuleCItemEffect::new(module_kind, inner));
                            all_effects.extend(effects);
                        }
                    }
                }

                Ok(all_effects)
            })
            .await?;

        // If we have effects to process, do it in a writable transaction
        self.db
            .write_with_expect_falliable(|dbtx| {
                for (module_id, module) in &*modules {
                    let module_dbtx = ModuleWriteTransactionCtx::new(*module_id, dbtx);

                    module
                        .process_effects(&module_dbtx, &effects)
                        .context(ProcessingEffectFailedSnafu {
                            module_id: *module_id,
                        })
                        .context(TxSnafu)?;
                }

                // Save the current position
                Self::save_cur_round_and_idx(dbtx, cur_round, cur_citem_idx)?;

                Ok(())
            })
            .await?;

        Ok(())
    }
}
