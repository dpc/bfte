use bfte_consensus_core::msg::{
    FinalityVoteUpdate, WaitFinalityVoteRequest, WaitFinalityVoteResponse,
    WaitNotarizedBlockRequest, WaitNotarizedBlockResponse, WaitVoteRequest, WaitVoteResponse,
};
use bfte_consensus_core::signed::Signed;
use bfte_util_error::WhateverResult;
use bfte_util_error::fmt::FmtCompact as _;
use iroh_dpc_rpc::{DpcRpc, RpcRead, RpcWrite};
use snafu::{ResultExt as _, whatever};
use tracing::{Level, debug, instrument, trace};

use crate::Node;
use crate::handle::{NodeHandle, NodeRefResultExt as _};
use crate::peer_address::AddressUpdate;
use crate::rpc::{
    GetBlockRequest, GetBlockResponse, GetConsensusVersionRequest, GetPeerAddressRequest,
    GetPeerAddressResponse, RPC_ID_GET_BLOCK, RPC_ID_GET_CONSENSUS_PARAMS,
    RPC_ID_GET_PEER_ADDR_UPDATE, RPC_ID_HELLO, RPC_ID_PUSH_PEER_ADDR_UPDATE,
    RPC_ID_WAIT_FINALITY_VOTE, RPC_ID_WAIT_NOTARIZED_BLOCK, RPC_ID_WAIT_VOTE,
};

const LOG_TARGET: &str = "bfte::node::rpc::server";

#[derive(Clone)]
pub(crate) struct RpcServer {
    handle: NodeHandle,
}

impl RpcServer {
    pub fn new(handle: NodeHandle) -> Self {
        Self { handle }
    }

    pub fn into_iroh_protocol_handler(self) -> impl iroh::protocol::ProtocolHandler {
        DpcRpc::builder(self)
            .handler(RPC_ID_HELLO, Self::handle_hello)
            .handler(RPC_ID_WAIT_VOTE, Self::handle_wait_vote)
            .handler(RPC_ID_WAIT_FINALITY_VOTE, Self::handle_wait_finality_vote)
            .handler(
                RPC_ID_WAIT_NOTARIZED_BLOCK,
                Self::handle_wait_notarized_block,
            )
            .handler(
                RPC_ID_PUSH_PEER_ADDR_UPDATE,
                Self::handle_push_peer_addr_update,
            )
            .handler(
                RPC_ID_GET_PEER_ADDR_UPDATE,
                Self::handle_get_peer_addr_update,
            )
            .handler(
                RPC_ID_GET_CONSENSUS_PARAMS,
                Self::handle_get_consensus_params,
            )
            .handler(RPC_ID_GET_BLOCK, Self::handle_get_block)
            .build()
    }

    async fn handle_hello(self, send: RpcWrite, recv: RpcRead) {
        if let Err(err) = self.handle_hello_try(send, recv).await {
            debug!(target: LOG_TARGET, err = %err.fmt_compact(), "Failed handling request hello");
        }
    }

    async fn handle_hello_try(self, mut send: RpcWrite, mut recv: RpcRead) -> WhateverResult<()> {
        let msg = recv
            .read_message_raw()
            .await
            .whatever_context("Failed to read request")?;
        send.write_message_raw(&msg)
            .await
            .whatever_context("Failed to write response")?;
        Ok(())
    }

    #[instrument(
        target = LOG_TARGET,
        skip_all,
        ret(level = Level::TRACE)
    )]
    async fn handle_wait_vote(self, send: RpcWrite, recv: RpcRead) {
        trace!(target: LOG_TARGET, "Start handling wait_vote request");
        if let Err(err) = self.handle_wait_vote_try(send, recv).await {
            debug!(target: LOG_TARGET, err = %err.fmt_compact(), "Failed handling request wait_vote");
        }
        trace!(target: LOG_TARGET, "End handling wait_vote request");
    }

    async fn handle_wait_vote_try(
        self,
        mut send: RpcWrite,
        mut recv: RpcRead,
    ) -> WhateverResult<()> {
        let req = recv
            .read_message_bincode::<WaitVoteRequest>()
            .await
            .whatever_context("Failed to read request")?;

        let req_round = req.round;
        let node_ref = &self.handle.node_ref().into_whatever()?;

        let Some(peer_pubkey) = node_ref.peer_pubkey else {
            whatever!("We have no peer pubkey")
        };

        let mut cur_round_rx = node_ref
            .consensus_wait()
            .await
            .current_round_with_timeout_rx();

        // We can't respond with a vote until we reached or passed a given round
        cur_round_rx
            .wait_for(|(cur_round, _)| req_round <= *cur_round)
            .await
            .whatever_context("Shutting down")?;

        let req_round_params = node_ref
            .consensus_wait()
            .await
            .get_round_params(req_round)
            .await;

        let Some(peer_idx) = req_round_params.find_peer_idx(peer_pubkey) else {
            whatever!("Not participating in this round");
        };

        let mut new_votes_rx = node_ref.consensus_wait().await.new_votes_rx();

        let resp = loop {
            if let Some(resp) = node_ref
                .consensus_wait()
                .await
                .get_vote(req_round, &req_round_params, peer_idx)
                .await
            {
                if !req.only_dummy || resp.block().is_dummy() {
                    break resp;
                }
            }

            if req_round < cur_round_rx.borrow().0 {
                // We either have a non-dummy vote and consensus, while request was for dummy
                // only, in which case we are not going to produce any
                // response ever, or we might have deleted old dummy votes
                // altogether.
                //
                // In any case, fail the request, and the requester should figure out everything
                // via notarized block request anyway.
                //
                // TODO: make a propoper response case? There's probably no reason
                // to send anything back anyway?
                whatever!("Not available");
            }

            trace!(target: LOG_TARGET, "Waiting for more votes");
            new_votes_rx
                .changed()
                .await
                .whatever_context("Shutting down")?;
            trace!(target: LOG_TARGET, "Got more votes");
        };

        send.write_message_bincode::<WaitVoteResponse>(&resp)
            .await
            .whatever_context("Write error")?;
        Ok(())
    }

    #[instrument(
        target = LOG_TARGET,
        skip_all,
        ret(level = Level::TRACE)
    )]
    async fn handle_wait_finality_vote(self, send: RpcWrite, recv: RpcRead) {
        trace!(target: LOG_TARGET, "Start handling wait_finality_vote request");
        if let Err(err) = self.handle_wait_finality_vote_try(send, recv).await {
            debug!(target: LOG_TARGET, err = %err.fmt_compact(), "Failed handling request wait_finality_vote");
        }
        trace!(target: LOG_TARGET, "End handling wait_finality_vote request");
    }

    async fn handle_wait_finality_vote_try(
        self,
        mut send: RpcWrite,
        mut recv: RpcRead,
    ) -> WhateverResult<()> {
        let req = recv
            .read_message_bincode::<WaitFinalityVoteRequest>()
            .await
            .whatever_context("Failed to read request")?;

        let req_round = req.round;

        let node_ref = &self.handle.node_ref().into_whatever()?;

        let Some(_peer_pubkey) = node_ref.peer_pubkey else {
            whatever!("We have no peer pubkey")
        };

        let seckey = node_ref.get_peer_secret_expect();

        let mut finality_self_vote_rx = node_ref.consensus_wait().await.finality_self_vote_rx();

        let finality_self_vote = *finality_self_vote_rx
            .wait_for(|finality_self_vote| req_round < *finality_self_vote)
            .await
            .whatever_context("Shutting down")?;

        send.write_message_bincode::<WaitFinalityVoteResponse>(&WaitFinalityVoteResponse {
            update: Signed::new_sign(FinalityVoteUpdate(finality_self_vote), seckey),
        })
        .await
        .whatever_context("Write error")?;
        Ok(())
    }

    #[instrument(
        target = LOG_TARGET,
        skip_all,
        ret(level = Level::TRACE)
    )]
    async fn handle_wait_notarized_block(self, send: RpcWrite, recv: RpcRead) {
        trace!(target: LOG_TARGET, "Start handling wait_notarized_block request");
        if let Err(err) = self.handle_wait_notarized_block_try(send, recv).await {
            debug!(target: LOG_TARGET, err = %err.fmt_compact(), "Failed handling request wait_notarized_block");
        }
        trace!(target: LOG_TARGET, "End handling wait_notarized_block request");
    }

    async fn handle_wait_notarized_block_try(
        self,
        mut send: RpcWrite,
        mut recv: RpcRead,
    ) -> WhateverResult<()> {
        let req = recv
            .read_message_bincode::<WaitNotarizedBlockRequest>()
            .await
            .whatever_context("Failed to read request")?;

        let node_ref = &self.handle.node_ref().into_whatever()?;

        let min_notarized_round = req.min_notarized_round;

        let mut cur_round_rx = node_ref
            .consensus_wait()
            .await
            .current_round_with_timeout_rx();

        // We can't respond with a vote until we reached or passed a given round
        cur_round_rx
            .wait_for(|(cur_round, _)| min_notarized_round <= *cur_round)
            .await
            .whatever_context("Shutting down")?;

        let resp = loop {
            if let Some(resp) = node_ref
                .consensus_wait()
                .await
                .get_notarized_block_resp(req)
                .await
            {
                break resp;
            }

            trace!(target: LOG_TARGET, "Waiting for more rounds");
            cur_round_rx
                .changed()
                .await
                .whatever_context("Shutting down")?;
            trace!(target: LOG_TARGET, "Got more rounds");
        };

        send.write_message_bincode::<WaitNotarizedBlockResponse>(&resp)
            .await
            .whatever_context("Write error")?;
        Ok(())
    }

    #[instrument(
        target = LOG_TARGET,
        skip_all,
        ret(level = Level::TRACE)
    )]
    async fn handle_push_peer_addr_update(self, send: RpcWrite, recv: RpcRead) {
        if let Err(err) = self.handle_push_peer_addr_update_try(send, recv).await {
            debug!(target: LOG_TARGET, err = %err.fmt_compact(), "Failed handling request push_peer_addr_update");
        }
    }

    async fn handle_push_peer_addr_update_try(
        self,
        _send: RpcWrite,
        mut recv: RpcRead,
    ) -> WhateverResult<()> {
        let update: Signed<AddressUpdate> = recv
            .read_message_bincode()
            .await
            .whatever_context("Failed to read request")?;

        let node_ref = &self.handle.node_ref().into_whatever()?;

        Node::handle_address_update(node_ref.db(), update).await?;
        Ok(())
    }

    async fn handle_get_peer_addr_update(self, send: RpcWrite, recv: RpcRead) {
        if let Err(err) = self.handle_get_peer_addr_update_try(send, recv).await {
            debug!(target: LOG_TARGET, err = %err.fmt_compact(), "Failed handling request get_peer_addr_update");
        }
    }

    async fn handle_get_peer_addr_update_try(
        self,
        mut send: RpcWrite,
        mut recv: RpcRead,
    ) -> WhateverResult<()> {
        let req: GetPeerAddressRequest = recv
            .read_message_bincode()
            .await
            .whatever_context("Failed to read request")?;

        let node_ref = &self.handle.node_ref().into_whatever()?;

        let update = Node::get_peer_addr(node_ref.db(), req.peer_pubkey).await?;

        send.write_message_bincode(&GetPeerAddressResponse { update })
            .await
            .whatever_context("Failed to write response")?;

        Ok(())
    }

    async fn handle_get_consensus_params(self, send: RpcWrite, recv: RpcRead) {
        if let Err(err) = self.handle_get_consensus_params_try(send, recv).await {
            debug!(target: LOG_TARGET, err = %err.fmt_compact(), "Failed handling request get_consensus_params");
        }
    }

    async fn handle_get_consensus_params_try(
        self,
        mut send: RpcWrite,
        mut recv: RpcRead,
    ) -> WhateverResult<()> {
        let req: GetConsensusVersionRequest = recv
            .read_message_bincode()
            .await
            .whatever_context("Failed to read request")?;

        let node_ref = &self.handle.node_ref().into_whatever()?;

        let consensus_params = node_ref
            .consensus_wait()
            .await
            .get_consensus_params(req.round)
            .await;

        let raw = consensus_params.to_raw();
        let out_hash = send
            .write_message_bao(&raw.as_inner_slice())
            .await
            .whatever_context("Failed to write response")?;

        assert_eq!(out_hash.as_bytes(), &raw.hash().to_bytes());

        Ok(())
    }

    async fn handle_get_block(self, send: RpcWrite, recv: RpcRead) {
        trace!(target: LOG_TARGET, "Start handling get_block request");
        if let Err(err) = self.handle_get_block_try(send, recv).await {
            debug!(target: LOG_TARGET, err = %err.fmt_compact(), "Failed handling request get_block");
        }
        trace!(target: LOG_TARGET, "End handling get_block request");
    }

    async fn handle_get_block_try(
        self,
        mut send: RpcWrite,
        mut recv: RpcRead,
    ) -> WhateverResult<()> {
        let req = recv
            .read_message_bincode::<GetBlockRequest>()
            .await
            .whatever_context("Failed to read request")?;

        let node_ref = &self.handle.node_ref().into_whatever()?;

        if let Some(block) = node_ref
            .consensus_wait()
            .await
            .get_finalized_block(req.round)
            .await
        {
            send.write_message_bincode::<GetBlockResponse>(&GetBlockResponse { block })
                .await
                .whatever_context("Write error")?;
        }

        Ok(())
    }
}
