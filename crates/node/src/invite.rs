use bfte_invite::Invite;
use bfte_node_core::address::{IrohAddress, PeerAddress};
use bfte_util_error::WhateverResult;
use snafu::OptionExt as _;

use crate::Node;

impl Node {
    pub async fn generate_invite_code(&self) -> WhateverResult<Invite> {
        let consensus = self
            .consensus()
            .as_ref()
            .whatever_context("Consensus not initialized")?;
        let round = consensus.get_finality_consensus().await;
        let block = if let Some(round) = round.and_then(|r| r.prev()) {
            consensus.get_prev_notarized_block(round).await
        } else {
            None
        };
        let init_params = consensus.get_init_params().await;
        let iroh_addr: IrohAddress = self.iroh_endpoint().node_id().into();

        Ok(Invite {
            init_params: Some(init_params.hash_and_len()),
            pin: block.map(|b| (b.round, b.hash())),
            address: PeerAddress::Iroh(iroh_addr),
        })
    }
}
