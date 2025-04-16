use std::future;
use std::sync::Arc;
use std::time::Duration;

use backon::Retryable as _;
use bfte_consensus::vote_set::VoteSet;
use bfte_consensus_core::block::{BlockHeader, BlockRound};
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::msg::{
    WaitNotarizedBlockRequest, WaitNotarizedBlockResponse, WaitVoteRequest, WaitVoteResponse,
};
use bfte_consensus_core::peer::{PeerIdx, PeerPubkey};
use bfte_consensus_core::signed::Signed;
use bfte_util_core::is_env_var_set;
use bfte_util_error::WhateverResult;
use bfte_util_error::fmt::FmtCompact as _;
use bfte_util_fmt_opt::AsFmtOption as _;
use iroh_dpc_rpc::bincode::RpcExtBincode as _;
use snafu::{ResultExt as _, Whatever};
use tokio::task::JoinSet;
use tracing::{debug, info, instrument, trace, warn};

use crate::rpc::{RPC_ID_WAIT_NOTARIZED_BLOCK, RPC_ID_WAIT_VOTE};
use crate::{ConnectionPool, LOG_TARGET, Node, RPC_BACKOFF};

impl Node {
    pub async fn run_consensus(self: Arc<Self>) {
        loop {
            self.run_consensus_round().await.expect("Consensus failure");
        }
    }

    /// Run a consensus for a given round
    ///
    /// Notably, this is the main place that drives state changes in the
    /// [`Self::consensus`],.
    ///
    /// It checks the round starting conditions, then spawn necessary tasks to
    /// produce [`RoundEvent`]s, then delivers them to the [`Self::consensus`]
    /// in a loop until the round is finished.
    pub(crate) async fn run_consensus_round(self: &Arc<Self>) -> WhateverResult<()> {
        let (round, params) = self.consensus.get_current_round_and_params().await;

        self.run_consensus_round_inner(round, params).await
    }

    #[instrument(
        name = "run_consensus_round"
        target = LOG_TARGET,
        level = "info",
        skip_all,
        fields(round = %round)
    )]
    pub(crate) async fn run_consensus_round_inner(
        self: &Arc<Self>,
        round: BlockRound,
        params: ConsensusParams,
    ) -> WhateverResult<()> {
        // Set of all the tasks specific for this round.
        //
        // Takes care of cancelling on drop. Each task produces (potentially)
        // as single `RoundEvent`
        let mut round_tasks = JoinSet::new();

        // In case we continuing previous round in progress, we might skip
        // certain tasks.
        let existing_dummy_votes = self.consensus.get_peers_with_dummy_votes(round).await;
        let existing_non_dummy_votes = self.consensus.get_peers_with_proposal_votes(round).await;

        // Our peer_idx in the round, None if we are not participating
        let our_peer_idx = self
            .peer_pubkey
            .and_then(|our_peer_pubkey| params.find_peer_idx(our_peer_pubkey));

        let finality_consensus = self
            .consensus
            .get_finality_consensus()
            .await
            .unwrap_or_default();

        let prev_notarized_block = self.consensus.get_prev_notarized_block(round).await;

        info!(
            target: LOG_TARGET,
            %round,
            seq = %prev_notarized_block.map(|b| b.seq).fmt_option(),
            cons_ver = %params.version,
            num_peers = %params.num_peers(),
            our_peer_idx = %our_peer_idx.fmt_option(),
            leader_idx = %round.leader_idx(params.num_peers()),
            %finality_consensus,
            "Running consensus round"
        );

        for (_, peer_pubkey) in params.iter_peers() {
            self.spawn_finality_vote_query_task(peer_pubkey).await;
        }

        self.run_consensus_round_spawn_generate_proposal_task(
            &mut round_tasks,
            our_peer_idx,
            round,
            &params,
            existing_dummy_votes,
            existing_non_dummy_votes,
        )
        .await;

        self.run_consensus_round_spawn_vote_proposal_task(
            &mut round_tasks,
            our_peer_idx,
            round,
            &params,
            existing_non_dummy_votes,
        )
        .await;

        self.run_consensus_round_spawn_vote_requests(
            &mut round_tasks,
            our_peer_idx,
            round,
            &params,
            existing_dummy_votes,
            existing_non_dummy_votes,
        )
        .await;

        self.run_consensus_round_spawn_notarized_blocks_requests(
            &mut round_tasks,
            our_peer_idx,
            round,
            &params,
            prev_notarized_block,
        )
        .await;

        self.run_consensus_round_spawn_timeout_vote_tasks(
            &mut round_tasks,
            our_peer_idx,
            round,
            &params,
            existing_dummy_votes,
        )
        .await;

        self.run_consensus_round_loop(round, round_tasks).await?;

        Ok(())
    }

    async fn run_consensus_round_spawn_generate_proposal_task(
        &self,
        round_tasks: &mut JoinSet<RoundEvent>,
        our_peer_idx: Option<PeerIdx>,
        round: BlockRound,
        params: &ConsensusParams,
        existing_dummy_votes: VoteSet,
        existing_non_dummy_votes: VoteSet,
    ) {
        let existing_votes = existing_dummy_votes | existing_non_dummy_votes;
        if let Some(our_peer_idx) = our_peer_idx {
            if round.leader_idx(params.num_peers()) == our_peer_idx {
                if !existing_votes.contains(our_peer_idx) {
                    round_tasks.spawn(
                        self.clone_strong()
                            .generate_proposal_round_task(round, our_peer_idx),
                    );
                }
            }
        }
    }

    async fn run_consensus_round_spawn_vote_requests(
        &self,
        round_tasks: &mut JoinSet<RoundEvent>,
        our_peer_idx: Option<PeerIdx>,
        round: BlockRound,
        params: &ConsensusParams,
        existing_dummy_votes: VoteSet,
        existing_non_dummy_votes: VoteSet,
    ) {
        for peer_idx in params.num_peers().peer_idx_iter() {
            if our_peer_idx.is_some_and(|our| our == peer_idx) {
                // Skip requesting vote from yourself. Our own proposal
                // and timeout are delivered by specific tasks.
                trace!(
                    target: LOG_TARGET,
                    %round,
                    %peer_idx,
                    "Not requesting vote from self"
                );
                continue;
            }
            // Once a correct peer votes dummy, it can not meaningfully vote again
            if existing_dummy_votes.contains(peer_idx) {
                trace!(
                    target: LOG_TARGET,
                    %round,
                    %peer_idx,
                    "Already have a dummy vote for the round, not requesting it again"
                );
                continue;
            }
            round_tasks.spawn(Self::request_peer_vote(
                self.connection_pool.clone(),
                round,
                peer_idx,
                *params
                    .peers
                    .get(peer_idx.as_usize())
                    .expect("Must have an entry"),
                // if we already have a non-dummy vote, we only care about dummy ones
                existing_non_dummy_votes.contains(peer_idx),
            ));
        }
    }

    async fn run_consensus_round_spawn_notarized_blocks_requests(
        &self,
        round_tasks: &mut JoinSet<RoundEvent>,
        our_peer_idx: Option<PeerIdx>,
        round: BlockRound,
        params: &ConsensusParams,
        prev_notarized_block: Option<BlockHeader>,
    ) {
        for peer_idx in params.num_peers().peer_idx_iter() {
            if our_peer_idx.is_some_and(|our| our == peer_idx) {
                // Skip requesting notarized block from yourself. We can't
                // update ourself with something we already have.
                trace!(
                    target: LOG_TARGET,
                    %round,
                    %peer_idx,
                    "Not requesting notarized block from self"
                );
                continue;
            }

            round_tasks.spawn(Self::request_peer_notarized_block(
                self.connection_pool.clone(),
                round,
                peer_idx,
                *params
                    .peers
                    .get(peer_idx.as_usize())
                    .expect("Must have an entry"),
                prev_notarized_block,
            ));
        }
    }

    async fn run_consensus_round_spawn_timeout_vote_tasks(
        &self,
        round_tasks: &mut JoinSet<RoundEvent>,
        our_peer_idx: Option<PeerIdx>,
        round: BlockRound,
        params: &ConsensusParams,
        existing_dummy_votes: VoteSet,
    ) {
        let current_round_with_timeout_start_rx =
            self.consensus.current_round_with_timeout_start_rx();

        if let Some(our_peer_idx) = our_peer_idx {
            if !existing_dummy_votes.contains(our_peer_idx) {
                // Pre-sign timeout vote, but release it later
                let dummy_vote = Signed::new_sign(
                    BlockHeader::new_dummy(round, params),
                    self.get_peer_secret_expect(),
                );
                let mut current_round_with_timeout_start_rx =
                    current_round_with_timeout_start_rx.clone();

                round_tasks.spawn({
                    async move {
                        let Ok((_, duration)) = current_round_with_timeout_start_rx
                            .wait_for(|(r, t)| {
                                *r == round && (t.is_some() || is_env_var_set("BFTE_FORCE_TIMEOUT"))
                            })
                            .await
                            .map(|o| *o)
                        else {
                            future::pending().await
                        };
                        let duration = duration.unwrap_or(Duration::from_secs(2));

                        debug!(
                            target: LOG_TARGET,
                            duration_millis = duration.as_millis(),
                            "Starting round timeout"
                        );
                        tokio::time::sleep(duration).await;
                        RoundEvent::VoteSelfTimeout {
                            resp: WaitVoteResponse::Vote { block: dummy_vote },
                            peer_idx: our_peer_idx,
                        }
                    }
                });
            }
        }
    }

    async fn run_consensus_round_spawn_vote_proposal_task(
        &self,
        round_tasks: &mut JoinSet<RoundEvent>,
        our_peer_idx: Option<PeerIdx>,
        round: BlockRound,
        params: &ConsensusParams,
        existing_non_dummy_votes: VoteSet,
    ) {
        if let Some(our_peer_idx) = our_peer_idx {
            if !existing_non_dummy_votes.contains(our_peer_idx)
                && round.leader_idx(params.num_peers()) != our_peer_idx
            {
                let mut new_proposal_rx = self.consensus.new_proposal_rx();
                let consensus = self.consensus.clone();
                let our_seckey = self.get_peer_secret_expect();
                round_tasks.spawn({
                    async move {
                        loop {
                            if let Some(proposal) = consensus.get_proposal(round).await {
                                let signed = Signed::new_sign(proposal, our_seckey);

                                debug!(
                                    target: LOG_TARGET,
                                    block = %proposal.hash(),
                                    "Voting on the current proposal"
                                );
                                return RoundEvent::VoteSelf {
                                    resp: WaitVoteResponse::Vote { block: signed },
                                    peer_idx: our_peer_idx,
                                };
                            }

                            let _ = new_proposal_rx.changed().await;
                        }
                    }
                });
            }
        }
    }

    async fn run_consensus_round_loop(
        &self,
        cur_round: BlockRound,
        mut round_tasks: JoinSet<RoundEvent>,
    ) -> WhateverResult<()> {
        debug!(
            target: LOG_TARGET,
            "Running consensus round loop"
        );

        let current_round_rx = self.consensus.current_round_with_timeout_start_rx();
        loop {
            if current_round_rx.borrow().0 != cur_round {
                break Ok(());
            }
            let event = round_tasks
                .join_next()
                .await
                .expect("Always has events")
                .whatever_context("Round logic failed")?;
            trace!(
                target: LOG_TARGET,
                ?event,
                "Consensus round loop event"
            );
            match event {
                RoundEvent::Vote {
                    peer_idx,
                    resp,
                    round,
                    peer_pubkey,
                } => {
                    let is_dummy = resp.block().is_dummy();
                    if let Err(err) = self.consensus.process_vote_response(peer_idx, resp).await {
                        warn!(
                            target: LOG_TARGET,
                            %peer_idx,
                            err = %err.fmt_compact(),
                            "Failed to process vote response")
                    }
                    if !is_dummy {
                        round_tasks.spawn(Self::request_peer_vote(
                            self.connection_pool.clone(),
                            round,
                            peer_idx,
                            peer_pubkey,
                            // if we already have a non-dummy vote, we only care about dummy ones
                            true,
                        ));
                    }
                }

                RoundEvent::VoteSelfProposal { peer_idx, resp } => {
                    if let Err(err) = self.consensus.process_vote_response(peer_idx, resp).await {
                        warn!(
                            target: LOG_TARGET,
                            %peer_idx,
                            err = %err.fmt_compact(),
                            "Failed to process self-proposal")
                    }
                }

                RoundEvent::VoteSelf { peer_idx, resp } => {
                    if let Err(err) = self.consensus.process_vote_response(peer_idx, resp).await {
                        warn!(
                            target: LOG_TARGET,
                            %peer_idx,
                            err = %err.fmt_compact(),
                            "Failed to process self-vote")
                    }
                }
                RoundEvent::VoteSelfTimeout { resp, peer_idx } => {
                    if let Err(err) = self.consensus.process_vote_response(peer_idx, resp).await {
                        warn!(
                            target: LOG_TARGET,
                            %peer_idx,
                            err = %err.fmt_compact(),
                            "Failed to process vote self-timeout vote")
                    }
                }

                RoundEvent::Notarized { peer_idx, resp } => {
                    if let Err(err) = self
                        .consensus
                        .process_notarized_block_response(peer_idx, resp)
                        .await
                    {
                        warn!(
                            target: LOG_TARGET,
                            %peer_idx,
                            err = %err.fmt_compact(),
                            "Failed to process notarized block response")
                    }
                }
            }
        }
    }

    async fn generate_proposal_round_task(
        self: Arc<Self>,
        round: BlockRound,
        our_peer_idx: PeerIdx,
    ) -> RoundEvent {
        let (block, payload) = self.consensus.generate_proposal(round).await;
        debug!(
            target: LOG_TARGET,
            hash = %block.hash(),
            "Proposing block"
        );
        let seckey = self.get_peer_secret_expect();
        let resp = WaitVoteResponse::Proposal {
            block: Signed::new_sign(block, seckey),
            payload,
        };

        RoundEvent::VoteSelfProposal {
            resp,
            peer_idx: our_peer_idx,
        }
    }

    async fn request_peer_vote(
        connection_pool: ConnectionPool,
        round: BlockRound,
        peer_idx: PeerIdx,
        peer_pubkey: PeerPubkey,
        only_dummy: bool,
    ) -> RoundEvent {
        {
            debug!(target: LOG_TARGET, %peer_idx, %peer_pubkey, %only_dummy, "Requesting vote from peer");
            || async {
                let mut conn = connection_pool
                    .connect(peer_pubkey)
                    .await
                    .whatever_context("Failed to connect to peer")?;
                trace!(target: LOG_TARGET, %peer_idx, %peer_pubkey, %only_dummy, "Making RPC for vote from peer");
                let resp = conn
                    .make_request_response_bincode(
                        RPC_ID_WAIT_VOTE,
                        WaitVoteRequest { round, only_dummy },
                    )
                    .await
                    .whatever_context("Failed wait vote request")?;
                trace!(target: LOG_TARGET, %peer_idx, %peer_pubkey, %only_dummy, "Got vote from peer");
                Ok(RoundEvent::Vote {
                    peer_idx,
                    resp,
                    round,
                    peer_pubkey,
                })
            }
        }
        .retry(RPC_BACKOFF)
        .notify(|err: &Whatever, dur: Duration| {
            debug!(target:
                LOG_TARGET,
                dur_millis = %dur.as_millis(),
                err = %err.fmt_compact(),
                "Retrying rpc"
            );
        })
        .await
        .expect("Always retry")
    }

    async fn request_peer_notarized_block(
        connection_pool: ConnectionPool,
        round: BlockRound,
        peer_idx: PeerIdx,
        peer_pubkey: PeerPubkey,
        prev_notarized_block: Option<BlockHeader>,
    ) -> RoundEvent {
        debug!(target: LOG_TARGET, %peer_idx, %peer_pubkey, "Requesting notarized block from peer");
        {
            || async {
                let mut conn = connection_pool
                    .connect(peer_pubkey)
                    .await
                    .whatever_context("Failed to connect to peer")?;
                let resp = conn
                    .make_request_response_bincode(
                        RPC_ID_WAIT_NOTARIZED_BLOCK,
                        WaitNotarizedBlockRequest {
                            cur_round: round,
                            min_notarized_round: prev_notarized_block
                                .map(|b| b.round.next().expect("Can't fail"))
                                .unwrap_or_default(),
                        },
                    )
                    .await
                    .whatever_context("Notarized block rpc failed")?;
                Ok(RoundEvent::Notarized { peer_idx, resp })
            }
        }
        .retry(RPC_BACKOFF)
        .notify(|err: &Whatever, dur: Duration| {
            debug!(target:
                LOG_TARGET,
                dur_millis = %dur.as_millis(),
                err = %err.fmt_compact(),
                "Retrying rpc"
            );
        })
        .await
        .expect("Always retry")
    }
}

#[derive(Debug)]
pub(crate) enum RoundEvent {
    /// Vote response from one of the peers, or ourselves
    Vote {
        resp: WaitVoteResponse,

        round: BlockRound,
        peer_idx: PeerIdx,
        peer_pubkey: PeerPubkey,
    },
    /// Our own proposal
    VoteSelfProposal {
        resp: WaitVoteResponse,

        peer_idx: PeerIdx,
    },
    /// Our own vote for someone's proposal
    VoteSelf {
        resp: WaitVoteResponse,

        peer_idx: PeerIdx,
    },
    /// Our own dummy (timeout) vote
    VoteSelfTimeout {
        resp: WaitVoteResponse,
        peer_idx: PeerIdx,
    },
    /// Block round notarized response from one of the peers
    Notarized {
        peer_idx: PeerIdx,
        resp: WaitNotarizedBlockResponse,
    },
}
