// SPDX-License-Identifier: MIT

//! Application-level consensus
//!
//! The initial and most important BFTE module. Every federation
//! starts with this module enabled. In this way it is somehow
//! hardcoded and special, but thanks to being a module, it operates
//! in the same framework as every other module.
//!
//! Tracks:
//!
//! * which modules are enabled,
//! * every module config (kind, consensus version, parameters)
//! * peer set
//! * votes on changing peer set and module configs

pub mod citem;
pub mod effects;
pub mod init;
pub mod module;
pub mod params;

pub use self::init::*;
pub use self::module::*;

mod tables;

#[cfg(test)]
mod tests;

use bfte_consensus_core::module::ModuleKind;
use bfte_consensus_core::ver::{ConsensusVersionMajor, ConsensusVersionMinor};
use bfte_module::kinds;

pub const KIND: ModuleKind = kinds::MODULE_KIND_CONSENSUS_CTRL;
const CURRENT_VERSION_MAJOR: ConsensusVersionMajor = ConsensusVersionMajor::new(0);
const CURRENT_VERSION_MINOR: ConsensusVersionMinor = ConsensusVersionMinor::new(1);

pub(crate) const LOG_TARGET: &str = "bfte::module::consensus-ctrl";
