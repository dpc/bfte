use bfte_consensus_core::ver::{ConsensusVersion, ConsensusVersionMajor, ConsensusVersionMinor};

use super::Consensus;

impl Consensus {
    pub const VERSION: ConsensusVersion =
        ConsensusVersion::new_const(ConsensusVersionMajor::new(0), ConsensusVersionMinor::new(0));
}
