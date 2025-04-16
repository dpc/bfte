use bfte_invite::Invite;
use bfte_node_core::address::{IrohAddress, PeerAddress};

use crate::Node;

impl Node {
    pub async fn generate_invite_code(&self) -> Invite {
        let round = self.consensus.get_finality_consensus().await;
        let block = if let Some(round) = round.and_then(|r| r.prev()) {
            self.consensus.get_prev_notarized_block(round).await
        } else {
            None
        };
        let init_params = self.consensus.get_init_params().await;
        let iroh_addr: IrohAddress = self.iroh_endpoint.node_id().into();

        Invite {
            init_params: Some(init_params.hash_and_len()),
            pin: block.map(|b| (b.round, b.hash())),
            address: PeerAddress::Iroh(iroh_addr),
        }
    }
}
