use std::sync::Arc;

use bfte_consensus_core::block::BlockRound;
use bfte_consensus_core::module::ModuleId;
use bfte_consensus_core::peer::{PeerPubkey, PeerSeckey};
use bfte_consensus_core::peer_set::PeerSet;
use bfte_db::Database;
use bfte_module::effect::EffectKindExt;
use bfte_module::module::db::ModuleDatabase;
use bfte_module::module::{IModule, ModuleInit, ModuleInitArgs};
use bfte_util_error::BoxedErrorResult;

use crate::CURRENT_VERSION;
use crate::citem::CoreConsensusCitem;
use crate::effects::{AddPeerEffect, ConsensusParamsChange, RemovePeerEffect};
use crate::init::CoreConsensusModuleInit;
use crate::module::CoreConsensusModule;

struct TestSetup {
    pub module: Arc<dyn IModule + Send + Sync>,
    pub peer_pubkey: PeerPubkey,
}

struct MultiPeerTestSetup {
    pub module: Arc<dyn IModule + Send + Sync>,
    #[allow(dead_code)]
    pub peer_pubkeys: Vec<PeerPubkey>,
}

impl TestSetup {
    async fn bootstrap_single_peer() -> BoxedErrorResult<Self> {
        let peer_seckey = PeerSeckey::generate();
        let peer_pubkey = peer_seckey.pubkey();

        let db = Arc::new(Database::new_in_memory().await?);
        let module_id = ModuleId::new(0);
        let module_db = ModuleDatabase::new(module_id, db.clone());

        // Bootstrap the consensus module with the initial peer set
        let module_config = module_db
            .write_with_expect(|module_dbtx| {
                let module_init = CoreConsensusModuleInit;
                let peer_set: PeerSet = vec![peer_pubkey].into();
                module_init.bootstrap_consensus(module_dbtx, module_id, peer_set)
            })
            .await;

        // Create the module via CoreConsensusModuleInit::init
        // Note: init() will call init_db_tx which creates all needed tables
        let module_init = CoreConsensusModuleInit;
        let module = module_init
            .init(ModuleInitArgs::new(
                module_id,
                db.clone(),
                CURRENT_VERSION,
                module_config.params,
                Some(peer_pubkey),
            ))
            .await?;

        Ok(Self {
            module,
            peer_pubkey,
        })
    }

    /// Downcast the module to CoreConsensusModule for accessing concrete
    /// implementation methods
    fn core_module(&self) -> Arc<CoreConsensusModule> {
        Arc::downcast::<CoreConsensusModule>(self.module.clone())
            .expect("Module should be CoreConsensusModule")
    }
}

impl MultiPeerTestSetup {
    async fn bootstrap_with_peers(peer_pubkeys: Vec<PeerPubkey>) -> BoxedErrorResult<Self> {
        let db = Arc::new(Database::new_in_memory().await?);
        let module_id = ModuleId::new(0);
        let module_db = ModuleDatabase::new(module_id, db.clone());

        // Bootstrap the consensus module with the initial peer set
        let module_config = module_db
            .write_with_expect(|module_dbtx| {
                let module_init = CoreConsensusModuleInit;
                let peer_set: PeerSet = peer_pubkeys.clone().into();
                module_init.bootstrap_consensus(module_dbtx, module_id, peer_set)
            })
            .await;

        // Create the module via CoreConsensusModuleInit::init
        // Note: init() will call init_db_tx which creates all needed tables
        let module_init = CoreConsensusModuleInit;
        let module = module_init
            .init(ModuleInitArgs::new(
                module_id,
                db.clone(),
                CURRENT_VERSION,
                module_config.params,
                Some(peer_pubkeys[0]), // Use first peer as the voting peer
            ))
            .await?;

        Ok(Self {
            module,
            peer_pubkeys,
        })
    }

    /// Downcast the module to CoreConsensusModule for accessing concrete
    /// implementation methods
    fn core_module(&self) -> Arc<CoreConsensusModule> {
        Arc::downcast::<CoreConsensusModule>(self.module.clone())
            .expect("Module should be CoreConsensusModule")
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_vote_add_peer_produces_effect() -> BoxedErrorResult<()> {
    let setup = TestSetup::bootstrap_single_peer().await?;

    // Create a new peer to add
    let new_peer_seckey = PeerSeckey::generate();
    let new_peer_pubkey = new_peer_seckey.pubkey();

    // Create a VoteAddPeer citem
    let vote_citem = CoreConsensusCitem::VoteAddPeer(new_peer_pubkey);
    let citem_raw = vote_citem.to_citem_raw();

    // Get current peer set (should only contain the initial peer)
    let peer_set: PeerSet = vec![setup.peer_pubkey].into();

    // Process the citem through the module using the IModule trait
    let effects = setup
        .core_module()
        .db
        .write_with_expect_falliable(|dbtx| {
            setup.module.process_citem(
                dbtx,
                BlockRound::from(0),
                setup.peer_pubkey,
                &peer_set,
                &citem_raw,
            )
        })
        .await?;

    // Check that exactly two effects were produced (AddPeerEffect + PeerSetChange)
    assert_eq!(effects.len(), 2, "Expected exactly two effects");

    // Check that the first effect is an AddPeerEffect with the correct peer
    let add_peer_effect: AddPeerEffect = EffectKindExt::decode(&effects[0])
        .map_err(|e| format!("Failed to decode AddPeerEffect: {e}"))?;

    assert_eq!(
        add_peer_effect.peer, new_peer_pubkey,
        "AddPeerEffect should contain the new peer's pubkey"
    );

    // Check that the second effect is a PeerSetChange with the updated peer set
    let peer_set_change_effect = ConsensusParamsChange::decode(&effects[1])
        .map_err(|e| format!("Failed to decode PeerSetChange: {e}"))?;

    assert_eq!(
        peer_set_change_effect.peer_set.len(),
        2,
        "PeerSetChange should contain 2 peers"
    );
    assert!(
        peer_set_change_effect.peer_set.contains(&setup.peer_pubkey),
        "PeerSetChange should contain the original peer"
    );
    assert!(
        peer_set_change_effect.peer_set.contains(&new_peer_pubkey),
        "PeerSetChange should contain the new peer"
    );

    // Verify that the new peer was actually added to the peers table
    // Use the module's get_peer_set method instead of poking at the database
    // directly
    let updated_peer_set = setup.core_module().get_peer_set().await;
    assert!(
        updated_peer_set.contains(&new_peer_pubkey),
        "New peer should be added to peer set"
    );
    assert!(
        updated_peer_set.contains(&setup.peer_pubkey),
        "Original peer should still be in peer set"
    );
    assert_eq!(
        updated_peer_set.len(),
        2,
        "Peer set should now contain exactly 2 peers"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_two_peers_voting_to_add_third() -> BoxedErrorResult<()> {
    // Create two initial peers
    let peer1_seckey = PeerSeckey::generate();
    let peer1_pubkey = peer1_seckey.pubkey();
    let peer2_seckey = PeerSeckey::generate();
    let peer2_pubkey = peer2_seckey.pubkey();

    let setup = MultiPeerTestSetup::bootstrap_with_peers(vec![peer1_pubkey, peer2_pubkey]).await?;

    // Create a new peer to add
    let new_peer_seckey = PeerSeckey::generate();
    let new_peer_pubkey = new_peer_seckey.pubkey();

    // Create a VoteAddPeer citem
    let vote_citem = CoreConsensusCitem::VoteAddPeer(new_peer_pubkey);
    let citem_raw = vote_citem.to_citem_raw();

    // Get current peer set (should contain both initial peers)
    let peer_set: PeerSet = vec![peer1_pubkey, peer2_pubkey].into();

    // First peer votes to add the new peer - should not reach threshold yet
    let effects1 = setup
        .core_module()
        .db
        .write_with_expect_falliable(|dbtx| {
            setup.module.process_citem(
                dbtx,
                BlockRound::from(0),
                peer1_pubkey,
                &peer_set,
                &citem_raw,
            )
        })
        .await?;

    // Should have no effects yet (threshold not reached)
    assert_eq!(
        effects1.len(),
        0,
        "No effects should be produced with only one vote"
    );

    // Verify that the new peer was NOT added yet
    let peer_set_after_first_vote = setup.core_module().get_peer_set().await;
    assert!(
        !peer_set_after_first_vote.contains(&new_peer_pubkey),
        "New peer should NOT be added after first vote"
    );
    assert_eq!(
        peer_set_after_first_vote.len(),
        2,
        "Peer set should still contain exactly 2 peers"
    );

    // Second peer votes to add the new peer - should reach threshold now
    let effects2 = setup
        .core_module()
        .db
        .write_with_expect_falliable(|dbtx| {
            setup.module.process_citem(
                dbtx,
                BlockRound::from(0),
                peer2_pubkey,
                &peer_set,
                &citem_raw,
            )
        })
        .await?;

    // Should have exactly two effects now (threshold reached)
    assert_eq!(
        effects2.len(),
        2,
        "Expected exactly two effects after reaching threshold"
    );

    // Check that the first effect is an AddPeerEffect with the correct peer
    let add_peer_effect: AddPeerEffect = EffectKindExt::decode(&effects2[0])
        .map_err(|e| format!("Failed to decode AddPeerEffect: {e}"))?;

    assert_eq!(
        add_peer_effect.peer, new_peer_pubkey,
        "AddPeerEffect should contain the new peer's pubkey"
    );

    // Check that the second effect is a PeerSetChange with the updated peer set
    let peer_set_change_effect = ConsensusParamsChange::decode(&effects2[1])
        .map_err(|e| format!("Failed to decode PeerSetChange: {e}"))?;

    assert_eq!(
        peer_set_change_effect.peer_set.len(),
        3,
        "PeerSetChange should contain 3 peers"
    );
    assert!(
        peer_set_change_effect.peer_set.contains(&peer1_pubkey),
        "PeerSetChange should contain peer1"
    );
    assert!(
        peer_set_change_effect.peer_set.contains(&peer2_pubkey),
        "PeerSetChange should contain peer2"
    );
    assert!(
        peer_set_change_effect.peer_set.contains(&new_peer_pubkey),
        "PeerSetChange should contain the new peer"
    );

    // Verify that the new peer was actually added to the peers table
    let updated_peer_set = setup.core_module().get_peer_set().await;
    assert!(
        updated_peer_set.contains(&new_peer_pubkey),
        "New peer should be added to peer set"
    );
    assert!(
        updated_peer_set.contains(&peer1_pubkey),
        "First peer should still be in peer set"
    );
    assert!(
        updated_peer_set.contains(&peer2_pubkey),
        "Second peer should still be in peer set"
    );
    assert_eq!(
        updated_peer_set.len(),
        3,
        "Peer set should now contain exactly 3 peers"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_two_peers_voting_to_remove_one() -> BoxedErrorResult<()> {
    // Create two initial peers
    let peer1_seckey = PeerSeckey::generate();
    let peer1_pubkey = peer1_seckey.pubkey();
    let peer2_seckey = PeerSeckey::generate();
    let peer2_pubkey = peer2_seckey.pubkey();

    let setup = MultiPeerTestSetup::bootstrap_with_peers(vec![peer1_pubkey, peer2_pubkey]).await?;

    // Create a VoteRemovePeer citem to remove peer2
    let vote_citem = CoreConsensusCitem::VoteRemovePeer(peer2_pubkey);
    let citem_raw = vote_citem.to_citem_raw();

    // Get current peer set (should contain both initial peers)
    let peer_set: PeerSet = vec![peer1_pubkey, peer2_pubkey].into();

    // First peer votes to remove peer2 - should not reach threshold yet
    let effects1 = setup
        .core_module()
        .db
        .write_with_expect_falliable(|dbtx| {
            setup.module.process_citem(
                dbtx,
                BlockRound::from(0),
                peer1_pubkey,
                &peer_set,
                &citem_raw,
            )
        })
        .await?;

    // Should have no effects yet (threshold not reached)
    assert_eq!(
        effects1.len(),
        0,
        "No effects should be produced with only one vote"
    );

    // Verify that peer2 was NOT removed yet
    let peer_set_after_first_vote = setup.core_module().get_peer_set().await;
    assert!(
        peer_set_after_first_vote.contains(&peer2_pubkey),
        "Peer2 should NOT be removed after first vote"
    );
    assert_eq!(
        peer_set_after_first_vote.len(),
        2,
        "Peer set should still contain exactly 2 peers"
    );

    // Second peer (peer2) votes to remove itself - should reach threshold now
    let effects2 = setup
        .core_module()
        .db
        .write_with_expect_falliable(|dbtx| {
            setup.module.process_citem(
                dbtx,
                BlockRound::from(0),
                peer2_pubkey,
                &peer_set,
                &citem_raw,
            )
        })
        .await?;

    // Should have exactly two effects now (threshold reached)
    assert_eq!(
        effects2.len(),
        2,
        "Expected exactly two effects after reaching threshold"
    );

    // Check that the first effect is a RemovePeerEffect with the correct peer
    let remove_peer_effect: RemovePeerEffect = EffectKindExt::decode(&effects2[0])
        .map_err(|e| format!("Failed to decode RemovePeerEffect: {e}"))?;

    assert_eq!(
        remove_peer_effect.peer, peer2_pubkey,
        "RemovePeerEffect should contain the removed peer's pubkey"
    );

    // Check that the second effect is a PeerSetChange with the updated peer set
    let peer_set_change_effect: ConsensusParamsChange = EffectKindExt::decode(&effects2[1])
        .map_err(|e| format!("Failed to decode PeerSetChange: {e}"))?;

    assert_eq!(
        peer_set_change_effect.peer_set.len(),
        1,
        "PeerSetChange should contain 1 peer after removal"
    );
    assert!(
        peer_set_change_effect.peer_set.contains(&peer1_pubkey),
        "PeerSetChange should contain only peer1 after removal"
    );
    assert!(
        !peer_set_change_effect.peer_set.contains(&peer2_pubkey),
        "PeerSetChange should not contain the removed peer"
    );

    // Verify that peer2 was actually removed from the peers table
    let updated_peer_set = setup.core_module().get_peer_set().await;
    assert!(
        !updated_peer_set.contains(&peer2_pubkey),
        "Peer2 should be removed from peer set"
    );
    assert!(
        updated_peer_set.contains(&peer1_pubkey),
        "Peer1 should still be in peer set"
    );
    assert_eq!(
        updated_peer_set.len(),
        1,
        "Peer set should now contain exactly 1 peer"
    );

    Ok(())
}
