use bfte_consensus_core::block::{BlockHash, BlockHeader, BlockRound};
use bfte_consensus_core::consensus_params::{
    ConsensusParamsHash, ConsensusParamsLen, ConsensusParamsRaw,
};
use bfte_consensus_core::msg::{WaitFinalityVoteRequest, WaitFinalityVoteResponse};
use bfte_consensus_core::peer::PeerPubkey;
use bfte_consensus_core::signed::{Notarized, Signed};
use bfte_consensus_core::ver::ConsensusVersion;
use bfte_util_error::WhateverResult;
use bincode::{Decode, Encode};
use iroh_dpc_rpc::RpcExt as _;
use iroh_dpc_rpc::bincode::RpcExtBincode as _;
use snafu::{ResultExt as _, whatever};

use crate::peer_address::AddressUpdate;

pub const RPC_ID_HELLO: u16 = 0x00;

// Consensus
pub const RPC_ID_WAIT_VOTE: u16 = 0x11;
pub const RPC_ID_WAIT_NOTARIZED_BLOCK: u16 = 0x12;
pub const RPC_ID_WAIT_FINALITY_VOTE: u16 = 0x13;

// Other
pub const RPC_ID_PUSH_PEER_ADDR_UPDATE: u16 = 0x20;
pub const RPC_ID_GET_PEER_ADDR_UPDATE: u16 = 0x21;
pub const RPC_ID_GET_BLOCK: u16 = 0x23;
pub const RPC_ID_GET_CONSENSUS_PARAMS: u16 = 0x24;
pub const RPC_ID_GET_CONSENSUS_VERSION: u16 = 0x25;

/// Wait for the peer vote on the block in the round
#[derive(Decode, Encode, Clone, Copy, PartialEq, Eq)]
pub struct GetBlockRequest {
    pub round: BlockRound,
}

#[derive(Decode, Encode, Clone)]
pub struct GetBlockResponse {
    pub block: Notarized<BlockHeader>,
}

/// Wait for the peer vote on the block in the round
#[derive(Decode, Encode, Clone, Copy, PartialEq, Eq)]
pub struct GetConsensusVersionRequest {
    pub round: BlockRound,
}
/// Wait for the peer vote on the block in the round
#[derive(Decode, Encode, Clone, Copy, PartialEq, Eq)]
pub struct GetConsensusVersionResponse {
    pub consensus_version: ConsensusVersion,
}

/// Wait for the peer vote on the block in the round
#[derive(Decode, Encode, Clone, Copy, PartialEq, Eq)]
pub struct GetConsensusParamsRequest {
    pub round: BlockRound,
}

/// Wait for the peer vote on the block in the round
#[derive(Decode, Encode, Clone, Copy, PartialEq, Eq)]
pub struct GetPeerAddressRequest {
    pub peer_pubkey: PeerPubkey,
}

/// Wait for the peer vote on the block in the round
#[derive(Decode, Encode, Clone)]
pub struct GetPeerAddressResponse {
    pub update: Option<Signed<AddressUpdate>>,
}

pub(crate) async fn get_block(
    conn: &mut iroh::endpoint::Connection,
    round: BlockRound,
) -> WhateverResult<Notarized<BlockHeader>> {
    let resp: Notarized<BlockHeader> = conn
        .make_request_response_bincode(RPC_ID_GET_BLOCK, GetBlockRequest { round })
        .await
        .whatever_context("Failed request get_block")?;

    if resp.round != round {
        whatever!(
            "Mismatched round block from peer: {} != {}",
            resp.round,
            round
        );
    }

    Ok(resp)
}

pub(crate) async fn get_block_hashed(
    conn: &mut iroh::endpoint::Connection,
    round: BlockRound,
    block_hash: BlockHash,
) -> WhateverResult<Notarized<BlockHeader>> {
    let resp = get_block(conn, round).await?;

    if resp.hash() != block_hash {
        whatever!("Mismatched hash block from peer");
    }

    Ok(resp)
}

#[allow(dead_code)]
pub(crate) async fn get_consensus_version(
    conn: &mut iroh::endpoint::Connection,
    round: BlockRound,
) -> WhateverResult<ConsensusVersion> {
    let resp: GetConsensusVersionResponse = conn
        .make_request_response_bincode(
            RPC_ID_GET_CONSENSUS_VERSION,
            GetConsensusVersionRequest { round },
        )
        .await
        .whatever_context("Failed request get_consensus_version")?;

    Ok(resp.consensus_version)
}

pub(crate) async fn get_consensus_params(
    conn: &mut iroh::endpoint::Connection,
    round: BlockRound,
    consensus_params_hash: ConsensusParamsHash,
    consensus_params_len: ConsensusParamsLen,
) -> WhateverResult<ConsensusParamsRaw> {
    conn.make_rpc_raw(
        RPC_ID_GET_CONSENSUS_PARAMS,
        move |mut w, mut r| async move {
            w.write_message_bincode(&GetConsensusParamsRequest { round })
                .await?;
            let resp = r
                .read_message_bao(
                    consensus_params_len.into(),
                    consensus_params_hash.to_bytes().into(),
                )
                .await?;

            Ok(ConsensusParamsRaw::from(resp))
        },
    )
    .await
    .whatever_context("Failed request get_consensus_params")
}

pub(crate) async fn get_peer_address(
    conn: &mut iroh::endpoint::Connection,
    peer_pubkey: PeerPubkey,
) -> WhateverResult<Option<Signed<AddressUpdate>>> {
    let resp: GetPeerAddressResponse = conn
        .make_request_response_bincode(
            RPC_ID_GET_PEER_ADDR_UPDATE,
            GetPeerAddressRequest { peer_pubkey },
        )
        .await
        .whatever_context("Failed request get_peer_address")?;

    if let Some(update) = resp.update {
        if update.inner.peer_pubkey != peer_pubkey {
            whatever!("Mismatched peer_pubkey");
        }
        update
            .verify_sig_peer_pubkey(peer_pubkey)
            .whatever_context("Invalid address update signature")?;

        return Ok(Some(update));
    }

    Ok(None)
}

pub(crate) async fn wait_finality_vote(
    conn: &mut iroh::endpoint::Connection,
    peer_pubkey: PeerPubkey,
    prev_vote: BlockRound,
) -> WhateverResult<WaitFinalityVoteResponse> {
    let resp: WaitFinalityVoteResponse = conn
        .make_request_response_bincode(
            RPC_ID_WAIT_FINALITY_VOTE,
            WaitFinalityVoteRequest { round: prev_vote },
        )
        .await
        .whatever_context("Failed request get_peer_address")?;

    resp.update
        .verify_sig_peer_pubkey(peer_pubkey)
        .whatever_context("Invalid address resp signature")?;

    Ok(resp)
}
