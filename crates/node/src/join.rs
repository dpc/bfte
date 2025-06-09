use std::sync::Arc;

use bfte_consensus::consensus::Consensus;
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::peer::PeerPubkey;
use bfte_db::Database;
use bfte_db::error::DbError;
use bfte_invite::Invite;
use bfte_node_core::address::PeerAddress;
use bfte_util_error::{Whatever, WhateverResult};
use iroh::endpoint::Connection;
use snafu::{ResultExt as _, Snafu, whatever};
use tracing::warn;

use crate::connection_pool::ALPN_BFTE_V0;
use crate::{LOG_TARGET, Node, rpc};

#[derive(Debug, Snafu)]
pub enum NodeJoinError {
    Db {
        source: DbError,
    },
    IrohEndpoint {
        source: anyhow::Error,
    },
    IrohConnection {
        source: anyhow::Error,
    },
    IrohAddress {
        source: Whatever,
    },
    PeerRequest {
        source: Whatever,
    },
    #[snafu(transparent)]
    ConsensusOpen {
        source: bfte_consensus::consensus::OpenError,
    },
}

pub type NodeJoinResult<T> = Result<T, NodeJoinError>;

impl Node {
    pub async fn consensus_join_static(
        db: Arc<Database>,
        invite: &Invite,
        our_peer_pubkey: Option<PeerPubkey>,
    ) -> NodeJoinResult<Consensus> {
        let iroh_endpoint = Self::make_iroh_endpoint(None)
            .await
            .context(IrohEndpointSnafu)?;

        let PeerAddress::Iroh(peer_iroh_addr) = invite.address;

        let mut conn = iroh_endpoint
            .connect(
                iroh_base::NodeId::try_from(peer_iroh_addr).context(IrohAddressSnafu)?,
                ALPN_BFTE_V0,
            )
            .await
            .context(IrohConnectionSnafu)?;
        let consensus = Self::consensus_join_static_inner(&mut conn, db, invite, our_peer_pubkey)
            .await
            .context(PeerRequestSnafu)?;

        Ok(consensus)
    }

    async fn consensus_join_static_inner(
        conn: &mut Connection,
        db: Arc<Database>,
        invite: &Invite,
        our_peer_pubkey: Option<PeerPubkey>,
    ) -> WhateverResult<Consensus> {
        let mut init_params = None;

        // Use the embedded init_params, to get initial consensus params for the
        // federation
        if let Some((consensus_params_hash, consensus_params_len)) = invite.init_params {
            let consensus_params = rpc::get_consensus_params(
                conn,
                0.into(),
                consensus_params_hash,
                consensus_params_len,
            )
            .await?;

            let consensus_params = ConsensusParams::from_raw(&consensus_params)
                .whatever_context("Failed to parse init consensus params")?;

            for peer_pubkey in consensus_params.peers.as_slice() {
                match rpc::get_peer_address(conn, *peer_pubkey).await? {
                    Some(update) => Self::handle_address_update(&db, update).await?,
                    None => {
                        warn!(target: LOG_TARGET, %peer_pubkey, "Missing other peer address");
                    }
                }
            }

            init_params = Some(consensus_params);
        }

        // Use the embedded pin (some recent block)
        if let Some((pin_round, pin_block_hash)) = invite.pin {
            let pin_block = rpc::get_block_hashed(conn, pin_round, pin_block_hash).await?;

            let pin_params = rpc::get_consensus_params(
                conn,
                pin_round,
                pin_block.consensus_params_hash,
                pin_block.consensus_params_len,
            )
            .await?;

            let pin_params = ConsensusParams::from_raw(&pin_params)
                .whatever_context("Failed to parse consensus params")?;

            for peer_pubkey in pin_params.peers {
                match rpc::get_peer_address(conn, peer_pubkey).await? {
                    Some(update) => Self::handle_address_update(&db, update).await?,
                    None => {
                        warn!(target: LOG_TARGET, %peer_pubkey, "Missing other peer address");
                    }
                }
            }

            // TODO: figure out `init_params` using chain of consensus_params,
            // if they were not present in the invite
        };

        let Some(init_params) = init_params else {
            whatever!("Init params not available in the invite");
        };

        Consensus::init(&init_params, db, our_peer_pubkey, invite.pin)
            .await
            .whatever_context("Failed to initialize consensus")
    }
}
