use bfte_consensus_core::block::{BlockHeader, BlockPayloadRaw, BlockRound};

use super::{Consensus, ConsensusReadDbOps as _};

impl Consensus {
    pub async fn generate_proposal(&self, round: BlockRound) -> (BlockHeader, BlockPayloadRaw) {
        let payload = self.generate_proposal_payload().await;
        let (consensus_params, prev_block) = self
            .db
            .read_with_expect(|ctx| {
                let consensus_params = ctx.get_consensus_params(round)?;
                let prev_block = ctx.get_prev_notarized_block(round)?;
                Ok((consensus_params, prev_block))
            })
            .await;
        (
            BlockHeader::builder()
                .maybe_prev(prev_block)
                .round(round)
                .consensus_params(&consensus_params)
                .payload(&payload)
                .build(),
            payload,
        )
    }
    pub async fn generate_proposal_payload(&self) -> BlockPayloadRaw {
        // TODO: generate
        BlockPayloadRaw::empty()
    }
}
