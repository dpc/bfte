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
use bfte_consensus_core::ver::{ConsensusVersion, ConsensusVersionMajor, ConsensusVersionMinor};

pub const KIND: ModuleKind = ModuleKind::new(0);
const CURRENT_VERSION: ConsensusVersion =
    ConsensusVersion::new_const(ConsensusVersionMajor::new(0), ConsensusVersionMinor::new(0));
