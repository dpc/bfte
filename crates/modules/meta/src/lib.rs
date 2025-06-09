// SPDX-License-Identifier: MIT

//! Meta module
//!
//! This module provides meta functionality for the BFTE system.

pub mod citem;
pub mod effects;
pub mod init;
pub mod module;

pub use self::init::*;
pub use self::module::*;

mod tables;

use bfte_consensus_core::module::ModuleKind;
use bfte_consensus_core::ver::{ConsensusVersionMajor, ConsensusVersionMinor};

pub const KIND: ModuleKind = ModuleKind::new(1);
const CURRENT_VERSION_MAJOR: ConsensusVersionMajor = ConsensusVersionMajor::new(0);
const CURRENT_VERSION_MINOR: ConsensusVersionMinor = ConsensusVersionMinor::new(0);
