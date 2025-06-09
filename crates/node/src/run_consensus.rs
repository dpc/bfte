use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::{env, future};

use backon::Retryable as _;
use bfte_consensus::vote_set::VoteSet;
use bfte_consensus_core::block::{BlockHeader, BlockPayloadRaw, BlockRound};
use bfte_consensus_core::citem::CItem;
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::msg::{
    WaitNotarizedBlockRequest, WaitNotarizedBlockResponse, WaitVoteRequest, WaitVoteResponse,
};
use bfte_consensus_core::peer::{PeerIdx, PeerPubkey};
use bfte_consensus_core::signed::Signed;
use bfte_consensus_core::timestamp::Timestamp;
use bfte_util_core::is_env_var_set;
use bfte_util_error::WhateverResult;
use bfte_util_error::fmt::FmtCompact as _;
use bfte_util_fmt_opt::AsFmtOption as _;
use iroh_dpc_rpc::bincode::RpcExtBincode as _;
use snafu::{ResultExt as _, Whatever};
use tokio::select;
use tokio::task::JoinSet;
use tokio::time::sleep;
use tracing::{debug, info, instrument, trace, warn};

use crate::connection_pool::ConnectionPool;
use crate::envs::BFTE_TEST_ROUND_DELAY;
use crate::rpc::{RPC_ID_WAIT_NOTARIZED_BLOCK, RPC_ID_WAIT_VOTE};
use crate::{LOG_TARGET, Node, RPC_BACKOFF};

impl Node {
    pub async fn run_consensus(self: Arc<Self>) {
        loop {
            self.run_consensus_round().await.expect("Consensus failure");
            if let Ok(delay) = env::var(BFTE_TEST_ROUND_DELAY) {
                let delay = u64::from_str(&delay)
                    .unwrap_or_else(|err| panic!("Invalid {BFTE_TEST_ROUND_DELAY}: {err}"));
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
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
        let (round, params) = self.consensus_expect().get_current_round_and_params().await;

        self.run_consensus_round_inner(round, params).await
    }

    #[instrument(
        target = LOG_TARGET,
        name = "run"
        level = "info",
        skip_all,
        fields(round = %round)
    )]
    pub(crate) async fn run_consensus_round_inner(
        self: &Arc<Self>,
        round: BlockRound,
        params: ConsensusParams,
    ) -> WhateverResult<()> {
        // Do not race too much ahead over what node-app was able to process.
        let Ok(_) = self
            .node_app_ack_rx
            .clone()
            .wait_for(|node_app_ack| {
                round
                    < node_app_ack
                        .checked_add(ConsensusParams::CONSENSUS_PARAMS_CORE_APPLY_DELAY_BASE)
                        .expect("Can't run out of u64 rounds ")
            })
            .await
        else {
            future::pending().await
        };

        let consensus = self.consensus_expect();

        // Set of all the tasks specific for this round.
        //
        // Takes care of cancelling on drop. Each task produces (potentially)
        // as single `RoundEvent`
        let mut round_tasks = JoinSet::new();

        // In case we continuing previous round in progress, we might skip
        // certain tasks.
        let existing_dummy_votes = consensus.get_peers_with_dummy_votes(round).await;
        let existing_non_dummy_votes = consensus.get_peers_with_proposal_votes(round).await;

        // Our peer_idx in the round, None if we are not participating
        let our_peer_idx = self
            .peer_pubkey
            .and_then(|our_peer_pubkey| params.find_peer_idx(our_peer_pubkey));

        let finality_consensus = self
            .consensus_expect()
            .get_finality_consensus()
            .await
            .unwrap_or_default();

        let prev_notarized_block = self
            .consensus_expect()
            .get_prev_notarized_block(round)
            .await;

        info!(
            target: LOG_TARGET,
            %round,
            prev_seq = %prev_notarized_block.map(|b| b.seq).fmt_option(),
            num_peers = %params.num_peers(),
            our_peer_idx = %our_peer_idx.fmt_option(),
            leader_idx = %round.leader_idx(params.num_peers()),
            %finality_consensus,
            "Running core consensus round…"
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
                if existing_votes.contains(our_peer_idx) {
                    info!(target: LOG_TARGET, "Already voted in this round. Will not generate proposal.");
                } else {
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
                self.connection_pool().clone(),
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
                self.connection_pool().clone(),
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
        let current_round_with_timeout_rx = self.consensus_expect().current_round_with_timeout_rx();

        if let Some(our_peer_idx) = our_peer_idx {
            if !existing_dummy_votes.contains(our_peer_idx) {
                // Pre-sign timeout vote, but release it later
                let dummy_vote = Signed::new_sign(
                    BlockHeader::new_dummy(round, params),
                    self.get_peer_secret_expect(),
                );

                let modules = self.weak_shared_modules.clone();
                let mut current_round_with_timeout_rx = current_round_with_timeout_rx.clone();
                let consensus = self.consensus_wait().await.clone();
                let finality_consensus_rx = consensus.finality_consensus_rx();
                let node_app_ack_rx = self.node_app_ack_rx.clone();

                round_tasks.spawn({
                    async move {
                        let wait_for_consensus_timeout_async = current_round_with_timeout_rx
                            .wait_for(|(r, timeout_enabled)| {
                                *r == round
                                    && (*timeout_enabled || is_env_var_set("BFTE_FORCE_TIMEOUT"))
                            });

                        // We cast a timeout vote if consensus tells us so, or
                        // we have own citems to broadcast
                        select! {
                            _ = wait_for_consensus_timeout_async => {
                                debug!(target: LOG_TARGET, "Starting round timeout due consensus state");
                            },
                            _= modules.wait_fresh_consensus_proposal(finality_consensus_rx, node_app_ack_rx) => {
                                debug!(target: LOG_TARGET, "Starting round timeout due to own pending citems")
                            },
                        };

                        let duration = consensus.get_current_round_timeout().await;

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
                let consensus = self.consensus_expect().clone();
                let mut new_proposal_rx = consensus.new_proposal_rx();
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
        let consensus = self.consensus_expect();
        trace!(
            target: LOG_TARGET,
            "Running consensus round loop…"
        );

        let current_round_rx = consensus.current_round_with_timeout_rx();
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
                    if let Err(err) = consensus.process_vote_response(peer_idx, resp).await {
                        warn!(
                            target: LOG_TARGET,
                            %peer_idx,
                            err = %err.fmt_compact(),
                            "Failed to process vote response")
                    }
                    if !is_dummy {
                        round_tasks.spawn(Self::request_peer_vote(
                            self.connection_pool().clone(),
                            round,
                            peer_idx,
                            peer_pubkey,
                            // if we already have a non-dummy vote, we only care about dummy ones
                            true,
                        ));
                    }
                }

                RoundEvent::VoteSelfProposal { peer_idx, resp } => {
                    if let Err(err) = consensus.process_vote_response(peer_idx, resp).await {
                        warn!(
                            target: LOG_TARGET,
                            %peer_idx,
                            err = %err.fmt_compact(),
                            "Failed to process self-proposal")
                    }
                }

                RoundEvent::VoteSelf { peer_idx, resp } => {
                    if let Err(err) = consensus.process_vote_response(peer_idx, resp).await {
                        warn!(
                            target: LOG_TARGET,
                            %peer_idx,
                            err = %err.fmt_compact(),
                            "Failed to process self-vote")
                    }
                }
                RoundEvent::VoteSelfTimeout { resp, peer_idx } => {
                    if let Err(err) = consensus.process_vote_response(peer_idx, resp).await {
                        warn!(
                            target: LOG_TARGET,
                            %peer_idx,
                            err = %err.fmt_compact(),
                            "Failed to process vote self-timeout vote")
                    }
                }

                RoundEvent::Notarized { peer_idx, resp } => {
                    if let Err(err) = consensus
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
        cur_round: BlockRound,
        our_peer_idx: PeerIdx,
    ) -> RoundEvent {
        let mut node_app_ack_rx = self.node_app_ack_rx.clone();
        let consensus = self.consensus_expect();
        let mut pending_transactions_rx = self.pending_transactions_rx.clone();
        let mut finality_consensus_rx = consensus.finality_consensus_rx();
        let mut finality_self_vote_rx = consensus.finality_self_vote_rx();

        // TODO: is this needed?
        node_app_ack_rx.mark_unchanged();
        finality_consensus_rx.mark_unchanged();
        pending_transactions_rx.mark_unchanged();

        let mut pending_citems: Vec<CItem> = Default::default();
        loop {
            if !pending_transactions_rx.borrow().is_empty() {
                break;
            }

            let wait_pending_params_change_async = async {
                // If consensus has any params changes pending, we want to produce
                // a round, even if empty, just to trigger the change in a timely maner.
                sleep(Duration::from_millis(20)).await;
                if !self
                    .consensus_wait()
                    .await
                    .has_pending_consensus_params_change(cur_round)
                    .await
                {
                    future::pending().await
                }
            };

            let mut finality_consensus_rx = finality_consensus_rx.clone();
            finality_consensus_rx.mark_unchanged();
            let finality_consensus_2_rx = finality_consensus_rx.clone();
            let node_app_ack_rx = node_app_ack_rx.clone();

            let wait_finality_self_vote_mismatch_async = async {
                sleep(Duration::from_millis(1)).await;

                if cur_round <= *finality_self_vote_rx.borrow() {
                    future::pending().await
                }
            };

            debug!(target: LOG_TARGET, %cur_round, "Awaiting new block proposal trigger…");
            select! {
                // We check proposed citems from our modules with priority,
                // and add pending transactions to this list.
                biased;

                // If the finality was increased, we want to wait for the node_app
                // to catch up to the new height immediately and retry
                _ = finality_consensus_rx.changed() => {
                    debug!(target: LOG_TARGET, "Finality consensus changed");
                    continue;
                },

                // If we get any pending citems, we're done
                //
                // Since we already have these citems, we record them
                // in a local variable and break.
                citems = self.weak_shared_modules.wait_fresh_consensus_proposal(finality_consensus_2_rx, node_app_ack_rx) => {

                    debug!(target: LOG_TARGET, "Got citems from modules");
                    if !citems.is_empty() {
                        pending_citems = citems;
                        break;
                    }

                    continue;
                },

                // If there are any pending transactions, we break.
                _res = pending_transactions_rx.changed() => {
                    debug!(target: LOG_TARGET, "Got pending transactions from node-app");
                    if _res.is_err() {
                        // If we're shutting down, just sleep and get dropped
                        future::pending().await
                    }

                    if !pending_transactions_rx.borrow().is_empty() {
                        break;
                    }

                    continue;
                },

                _ = wait_finality_self_vote_mismatch_async => {
                    // If the previous round was a dummy, we want to propose
                    // a block, even if empty, just so all the peers can agree
                    // on the notarization&finalization ASAP.
                    debug!(
                        target: LOG_TARGET,
                        "Proposing block, because previous round was a dummy"
                    );
                    break;
                }
                _ = wait_pending_params_change_async => {
                    debug!(
                        target: LOG_TARGET,
                        "Proposing block, just to advance params change"
                    );
                    break;
                }
            };
        }

        pending_citems.extend(
            pending_transactions_rx
                .borrow()
                .iter()
                .map(|tx| CItem::Transaction(tx.to_owned())),
        );

        debug!(target: LOG_TARGET, %cur_round, items = %pending_citems.len(), "Building new block proposal");
        let (block, payload) = self.generate_proposal(cur_round, &pending_citems).await;

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

    pub async fn generate_proposal(
        &self,
        round: BlockRound,
        citems: &[CItem],
    ) -> (BlockHeader, BlockPayloadRaw) {
        let consensus_params = self.consensus_expect().get_consensus_params(round).await;
        let prev_block = self
            .consensus_expect()
            .get_prev_notarized_block(round)
            .await;
        let payload = BlockPayloadRaw::encode_citems(citems);
        (
            BlockHeader::builder()
                .maybe_prev(prev_block)
                .round(round)
                .consensus_params(&consensus_params)
                .payload(&payload)
                .timestamp(Timestamp::now())
                .build(),
            payload,
        )
    }

    async fn request_peer_vote(
        connection_pool: ConnectionPool,
        round: BlockRound,
        peer_idx: PeerIdx,
        peer_pubkey: PeerPubkey,
        only_dummy: bool,
    ) -> RoundEvent {
        {
            debug!(
                target: LOG_TARGET,
                %peer_idx,
                %peer_pubkey,
                %only_dummy,
                %round,
                "Requesting vote from peer…"
            );
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
        debug!(
            target: LOG_TARGET,
            %peer_idx,
            %peer_pubkey,
            %round,
            "Requesting notarized block from peer…"
        );
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
