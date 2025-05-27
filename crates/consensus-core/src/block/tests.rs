use hex_literal::hex;

use crate::block::BlockHeader;
use crate::consensus_params::ConsensusParams;

#[test]
fn block_header_size_sanity() {
    let block = BlockHeader::new_dummy(0.into(), &ConsensusParams::new_test_dummy());
    assert_eq!(
        bincode::encode_to_vec(block, crate::bincode::STD_BINCODE_CONFIG)
            .expect("Can't fail")
            .len(),
        // Nice round number so it can potentially be compactly
        // mass-stored.
        128
    )
}

#[test]
fn block_header_fixture() {
    for (round, hash_fixture) in [
        (
            0,
            hex!("327fa2bc357718ab39fbf0c46173b82f531f4dc145929bb44f0d156b26625668"),
        ),
        (
            1,
            hex!("0b4b83e61d52d12832ff2cf9e293f66cf38b89a450ebd87de77bc3955dea9aca"),
        ),
    ] {
        let block = BlockHeader::new_dummy(round.into(), &ConsensusParams::new_test_dummy());
        let hash = block.hash().to_bytes();
        assert_eq!(
            hash,
            hash_fixture,
            "{round}: {} -> {}",
            data_encoding::HEXLOWER.encode_display(&hash_fixture),
            data_encoding::HEXLOWER.encode_display(&hash),
        );
    }
}
