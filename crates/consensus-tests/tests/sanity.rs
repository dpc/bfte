use bfte_consensus::consensus::Consensus;
use bfte_consensus_core::block::{BlockHeader, BlockPayloadRaw, BlockRound};
use bfte_consensus_core::consensus_params::ConsensusParams;
use bfte_consensus_core::msg::{
    FinalityVoteUpdate, WaitFinalityVoteResponse, WaitNotarizedBlockResponse, WaitVoteResponse,
};
use bfte_consensus_core::num_peers::NumPeers;
use bfte_consensus_core::peer::{PeerIdx, PeerPubkey, PeerSeckey};
use bfte_consensus_core::signed::{Notarized, Signable as _, Signed};
use bfte_consensus_core::timestamp::Timestamp;
use bfte_consensus_core::ver::ConsensusVersion;
use bfte_db::Database;
use bfte_util_error::{BoxedErrorResult, WhateverResult};
use snafu::ResultExt as _;

struct Setup {
    pub consensus: Consensus,
    pub cons_params: ConsensusParams,
    pub seckeys: Vec<PeerSeckey>,
}

impl Setup {
    async fn bootstrap(
        num_peers: NumPeers,
        init_core_module_cons_version: ConsensusVersion,
    ) -> WhateverResult<Self> {
        let mut seckeys: Vec<_> = (0..num_peers.total())
            .map(|_| PeerSeckey::generate())
            .collect();

        // PeerIdx's are assigned based on the pubkey, so sort the seckeys identifying
        // the peers for our convenience.
        seckeys.sort_unstable_by_key(|seckey1| seckey1.pubkey());

        let cons_params = ConsensusParams {
            prev_mid_block: None,
            peers: seckeys.iter().map(|s| s.pubkey()).collect(),
            consensus_params_format_version: ConsensusParams::FORMAT_VERSION,
            init_core_module_cons_version,
            timestamp: Timestamp::now(),
            schedule_round: 0.into(),
            apply_round: 0.into(),
        };
        let consensus = temp_consensus(&cons_params, Some(seckeys[0].pubkey()))
            .await
            .whatever_context("Failed to create temporary consensus")?;

        Ok(Self {
            consensus,
            cons_params,
            seckeys,
        })
    }

    fn seckey(&self) -> PeerSeckey {
        self.seckeys[0]
    }
}

pub(crate) async fn temp_consensus(
    params: &ConsensusParams,
    our_peer_pubkey: Option<PeerPubkey>,
) -> BoxedErrorResult<Consensus> {
    let db = Database::new_in_memory().await?;
    let consensus = Consensus::init(params, db.into(), our_peer_pubkey, None).await?;

    Ok(consensus)
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn starting_consensus_and_generating_first_block_via_vote() -> BoxedErrorResult<()> {
    let setup = Setup::bootstrap(1.into(), ConsensusVersion::new(0, 0)).await?;
    let payload = BlockPayloadRaw::empty();

    assert_eq!(
        BlockRound::from(0),
        setup.consensus.get_current_round().await
    );

    let block = BlockHeader::builder()
        .consensus_params(&setup.cons_params)
        .round(0.into())
        .payload(&BlockPayloadRaw::empty())
        .timestamp(Timestamp::ZERO)
        .build();

    setup
        .consensus
        .process_vote_response(
            PeerIdx::from(0),
            WaitVoteResponse::Proposal {
                block: Signed::new_sign(block, setup.seckey()),
                payload,
            },
        )
        .await?;

    assert_eq!(
        BlockRound::from(1),
        setup.consensus.get_current_round().await
    );

    assert_eq!(
        BlockRound::from(1),
        *setup.consensus.finality_consensus_rx().borrow()
    );

    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn starting_consensus_and_generating_first_block_via_notarization() -> BoxedErrorResult<()> {
    let setup = Setup::bootstrap(1.into(), ConsensusVersion::new(0, 0)).await?;
    let payload = BlockPayloadRaw::empty();

    assert_eq!(
        BlockRound::from(0),
        setup.consensus.get_current_round().await
    );

    let block = BlockHeader::builder()
        .payload(&BlockPayloadRaw::empty())
        .consensus_params(&setup.cons_params)
        .round(0.into())
        .timestamp(Timestamp::ZERO)
        .build();

    setup
        .consensus
        .process_notarized_block_response(
            PeerIdx::from(0),
            WaitNotarizedBlockResponse {
                block: Notarized::new(block, [(PeerIdx::from(0), block.sign_with(setup.seckey()))]),
                payload,
            },
        )
        .await?;

    assert_eq!(
        BlockRound::from(1),
        setup.consensus.get_current_round().await
    );

    assert_eq!(
        BlockRound::from(1),
        *setup.consensus.finality_consensus_rx().borrow()
    );

    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn two_peers_first_round() -> WhateverResult<()> {
    let num_peers: NumPeers = 2.into();
    let setup = Setup::bootstrap(num_peers, ConsensusVersion::new(0, 0)).await?;
    let payload = BlockPayloadRaw::empty();

    assert_eq!(BlockRound::ZERO.leader_idx(num_peers), 1.into());

    let block = BlockHeader::builder()
        .consensus_params(&setup.cons_params)
        .round(0.into())
        .payload(&BlockPayloadRaw::empty())
        .timestamp(Timestamp::ZERO)
        .build();

    setup
        .consensus
        .process_vote_response(
            PeerIdx::from(1),
            WaitVoteResponse::Proposal {
                block: Signed::new_sign(block, setup.seckeys[1]),
                payload,
            },
        )
        .await
        .whatever_context("Failed to process first proposal from peer 1")?;

    assert_eq!(
        BlockRound::from(0),
        setup.consensus.get_current_round().await
    );

    assert_eq!(
        BlockRound::from(0),
        *setup.consensus.finality_consensus_rx().borrow()
    );

    setup
        .consensus
        .process_vote_response(
            PeerIdx::from(0),
            WaitVoteResponse::Vote {
                block: Signed::new_sign(block, setup.seckeys[0]),
            },
        )
        .await
        .whatever_context("Failed to process first vote from peer 0")?;

    assert_eq!(
        BlockRound::from(1),
        setup.consensus.get_current_round().await
    );

    assert_eq!(
        BlockRound::from(0),
        *setup.consensus.finality_consensus_rx().borrow()
    );

    setup
        .consensus
        .process_finality_vote_update_response(
            setup.seckeys[1].pubkey(),
            WaitFinalityVoteResponse {
                update: Signed::new_sign(FinalityVoteUpdate::new(1.into()), setup.seckeys[1]),
            },
        )
        .await
        .whatever_context("Failed to process finality vote from peer 1")?;

    Ok(())
}
