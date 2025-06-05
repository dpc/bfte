use bfte_consensus_core::consensus_params::ConsensusParams;
use bincode::{Decode, Encode};

#[derive(Encode, Decode)]
pub struct CoreConsensusModuleParams {
    pub consensus_params: ConsensusParams,
}
