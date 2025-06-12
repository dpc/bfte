use std::collections::BTreeMap;

use bfte_consensus_core::block::{BlockHeader, BlockRound};
use bfte_consensus_core::citem::CItem;
use bfte_consensus_core::module::ModuleId;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_consensus_core::peer_set::PeerSet;
use bfte_consensus_core::timestamp::Timestamp;
use bfte_db::ctx::WriteTransactionCtx;
use bfte_db::error::{DbResult, TxSnafu};
use bfte_module::effect::{EffectKind as _, EffectKindExt as _, ModuleCItemEffect};
use bfte_module::module::config::ModuleConfig;
use bfte_module::module::db::ModuleWriteTransactionCtx;
use bfte_module_consensus_ctrl::effects::{
    AddModuleEffect, ConsensusParamsChange, ModuleVersionUpgradeEffect,
};
use bfte_util_error::Whatever;
use bfte_util_error::fmt::FmtCompact as _;
use snafu::{IntoError as _, OptionExt as _, ResultExt as _, Snafu};
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
        peer_pubkey: PeerPubkey,
        peer_set: &mut Option<PeerSet>,
        modules_configs: &mut Option<BTreeMap<ModuleId, ModuleConfig>>,
        citem: &CItem,
    ) {
        if let Err(err) = self
            .process_citem_try(
                (cur_round, cur_citem_idx),
                block_header.round,
                block_header.timestamp,
                peer_pubkey,
                peer_set.as_mut().expect("Must be set at this point"),
                modules_configs,
                citem,
            )
            .await
        {
            debug!(target: LOG_TARGET, err = %err.fmt_compact(), %cur_round, %cur_citem_idx, "Invalid consensus item" );
            // If processing failed, we need to reset peer_set and module_configs, as they
            // might have been altered, while the changes to consensus were
            // rolled back
            *peer_set = None;
            *modules_configs = None;

            // If processing failed, we need to advance the position in another, individual
            // dbtx, as the existing one was rolled back.
            self.db
                .write_with_expect(|dbtx| {
                    Self::save_cur_round_and_idx_dbtx(dbtx, cur_round, cur_citem_idx.next())
                })
                .await;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn process_citem_try(
        &self,
        (cur_round, cur_citem_idx): (BlockRound, BlockCItemIdx),
        block_round: BlockRound,
        block_timestamp: Timestamp,
        peer_pubkey: PeerPubkey,
        peer_set: &mut PeerSet,
        modules_configs: &mut Option<BTreeMap<ModuleId, ModuleConfig>>,
        citem: &CItem,
    ) -> ProcessCItemResult<()> {
        let modules = self.modules.read().await;

        // First, collect all effects with a read-only transaction
        self.db
            .write_with_expect_falliable(|dbtx| {
                let mut effects = Vec::with_capacity(8);

                match citem {
                    CItem::PeerCItem(module_citem) => {
                        let module_id = module_citem.module_id();
                        if !peer_set.contains(&peer_pubkey) {
                            None.whatever_context(
                                "Ignoring citem from a peer pending consensus removal",
                            )
                            .context(ProcessingCItemFailedSnafu { module_id })
                            .context(TxSnafu)?;
                        }
                        let module = modules
                            .get(&module_id)
                            .context(UnknownModuleIdSnafu { module_id })
                            .context(TxSnafu)?;
                        let module_kind = module.config.kind;

                        let module_dbtx = ModuleWriteTransactionCtx::new(module_id, dbtx);

                        effects.extend(
                            module
                                .process_citem(
                                    &module_dbtx,
                                    block_round,
                                    peer_pubkey,
                                    peer_set,
                                    module_citem.inner(),
                                )
                                .map_err(|db_tx_err| {
                                    db_tx_err.map(|e| {
                                        (ProcessingCItemFailedSnafu { module_id }).into_error(e)
                                    })
                                })?
                                .into_iter()
                                .map(|inner| ModuleCItemEffect::new(module_kind, inner)),
                        );
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

                            let module_dbtx = ModuleWriteTransactionCtx::new(module_id, dbtx);

                            effects.extend(
                                module
                                    .process_input(&module_dbtx, input.inner())
                                    .map_err(|db_tx_err| {
                                        db_tx_err.map(|e| {
                                            (ProcessingInputFailedSnafu { module_id }).into_error(e)
                                        })
                                    })?
                                    .into_iter()
                                    .map(|inner| ModuleCItemEffect::new(module_kind, inner)),
                            );
                        }

                        // Process all outputs
                        for output in &transaction.inner.outputs {
                            let module_id = output.module_id();
                            let module = modules
                                .get(&module_id)
                                .context(UnknownModuleIdSnafu { module_id })
                                .context(TxSnafu)?;
                            let module_kind = module.config.kind;

                            let module_dbtx = ModuleWriteTransactionCtx::new(module_id, dbtx);

                            effects.extend(
                                module
                                    .process_output(&module_dbtx, output.inner())
                                    .map_err(|db_tx_err| {
                                        db_tx_err.map(|e| {
                                            (ProcessingOutputFailedSnafu { module_id })
                                                .into_error(e)
                                        })
                                    })?
                                    .into_iter()
                                    .map(|inner| ModuleCItemEffect::new(module_kind, inner)),
                            );
                        }
                    }
                }

                self.process_consensus_change_effects_core_pre(
                    dbtx,
                    cur_round,
                    block_timestamp,
                    peer_set,
                    &effects,
                )?;
                for (&module_id, module) in &*modules {
                    let module_dbtx = ModuleWriteTransactionCtx::new(module_id, dbtx);

                    module
                        .process_effects(&module_dbtx, peer_set, &effects)
                        .map_err(|db_tx_err| {
                            db_tx_err
                                .map(|e| (ProcessingEffectFailedSnafu { module_id }).into_error(e))
                        })?;
                }

                self.process_consensus_change_effects_core_post(modules_configs, &effects)?;
                // Save the current position
                Self::save_cur_round_and_idx_dbtx(dbtx, cur_round, cur_citem_idx)?;

                Ok(())
            })
            .await?;

        Ok(())
    }

    /// Core consensus reacts to consensus changes changes dictate by the
    /// consensus ctrl module
    fn process_consensus_change_effects_core_pre(
        &self,
        dbtx: &WriteTransactionCtx,
        round: BlockRound,
        block_timestamp: Timestamp,
        peer_set: &mut PeerSet,
        effects: &[ModuleCItemEffect],
    ) -> DbResult<()> {
        for effect in effects {
            // Only process effects from our own module
            if effect.module_kind() != bfte_module_consensus_ctrl::KIND {
                continue;
            }

            if effect.inner().effect_id == ConsensusParamsChange::EFFECT_ID {
                // Decode the consensus change event
                let change =
                    ConsensusParamsChange::decode(effect.inner()).expect("Can't fail to decode");

                // Modify the copy of peer_set in memory (will get reverted if processing fails
                // later)
                *peer_set = change.peer_set.clone();

                // Process the change in the core consensus (dbtx will get rolled back if
                // processing fails later)
                self.consensus.consensus_params_change_tx(
                    dbtx,
                    round,
                    block_timestamp,
                    change.peer_set,
                )?;
            }
        }
        Ok(())
    }

    fn process_consensus_change_effects_core_post(
        &self,
        modules_configs: &mut Option<BTreeMap<ModuleId, ModuleConfig>>,
        effects: &[ModuleCItemEffect],
    ) -> DbResult<()> {
        for effect in effects {
            // Only process effects from our own module
            if effect.module_kind() != bfte_module_consensus_ctrl::KIND {
                continue;
            }

            if effect.inner().effect_id == AddModuleEffect::EFFECT_ID
                || effect.inner().effect_id == ModuleVersionUpgradeEffect::EFFECT_ID
            {
                // Just invalidate, so it gets re-read and reconfigured on next iteration
                *modules_configs = None;
            }
        }
        Ok(())
    }
}
