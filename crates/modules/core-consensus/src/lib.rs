// SPDX-License-Identifier: MIT

pub mod init;
pub mod module;
pub mod params;

pub use self::init::*;
pub use self::module::*;

mod tables;

use bfte_consensus_core::module::ModuleKind;
use bfte_consensus_core::ver::{ConsensusVersion, ConsensusVersionMajor, ConsensusVersionMinor};

pub const KIND: ModuleKind = ModuleKind::new(0);
const CURRENT_VERSION: ConsensusVersion =
    ConsensusVersion::new_const(ConsensusVersionMajor::new(0), ConsensusVersionMinor::new(0));
