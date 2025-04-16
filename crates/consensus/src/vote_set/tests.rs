use bfte_consensus_core::bincode::STD_BINCODE_CONFIG;
use bfte_util_bincode::decode_whole;

use crate::vote_set::VoteSet;

#[test]
pub(crate) fn vote_set_sanity() {
    let mut set = VoteSet::default();

    set.insert(3.into());
    assert!(set.contains(3.into()));
    assert!(!set.contains(4.into()));
    assert!(!set.contains(2.into()));
    set.insert(100.into());
    assert!(set.contains(100.into()));

    let rr_set = decode_whole(
        &bincode::encode_to_vec(set, STD_BINCODE_CONFIG).expect("Can't fail"),
        STD_BINCODE_CONFIG,
    )
    .expect("Can't fail");

    assert_eq!(set, rr_set);
}

#[test]
pub(crate) fn vote_set_sanity_2() {
    let mut set = VoteSet::default();

    set.insert(1.into());
    set.insert(3.into());

    assert_eq!(
        bincode::encode_to_vec(set, STD_BINCODE_CONFIG).expect("Can't fail"),
        vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 10
        ]
    );
    set.insert(13.into());
    set.insert(100.into());
    assert_eq!(
        bincode::encode_to_vec(set, STD_BINCODE_CONFIG).expect("Can't fail"),
        vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 32, 10
        ],
    );
    set.insert(255.into());
    set.insert(254.into());
    set.insert(253.into());
    set.insert(252.into());
    assert_eq!(
        bincode::encode_to_vec(set, STD_BINCODE_CONFIG).expect("Can't fail"),
        vec![
            0xf0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 32, 10
        ],
    );
}
