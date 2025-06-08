use hex_literal::hex;

use crate::block::BlockHeader;
use crate::consensus_params::ConsensusParams;

#[test]
fn block_header_size_sanity() {
    let block = BlockHeader::new_dummy(0.into(), &ConsensusParams::new_test_dummy());
    let encoded = bincode::encode_to_vec(block, crate::bincode::CONSENSUS_BINCODE_CONFIG)
        .expect("Can't fail");
    assert_eq!(
        encoded.len(),
        // Nice round number so it can potentially be compactly
        // mass-stored.
        128,
        "Size mismatch {}",
        data_encoding::HEXLOWER.encode_display(&encoded),
    )
}

#[test]
fn block_header_fixture() {
    for (round, hash_fixture) in [
        (
            0,
            hex!("6724e30b6f8b84d9caa9a777fe41662400a8afdd2134b59a00a0a9cb72d98e90"),
        ),
        (
            1,
            hex!("14ef6ce40bf4d1d5204bf9c7a8d6b7b087bdadaf807a886131f8841eb7fedf72"),
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
