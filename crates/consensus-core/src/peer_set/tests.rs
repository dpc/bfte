use super::PeerSet;
use crate::peer::PeerSeckey;

#[test]
fn peer_set_sanity() {
    let mut set = PeerSet::new();

    let pk1 = PeerSeckey::generate().pubkey();
    let pk2 = PeerSeckey::generate().pubkey();
    let pk3 = PeerSeckey::generate().pubkey();

    assert!(!set.remove(pk1));

    assert!(set.insert(pk1));
    assert!(!set.insert(pk1));

    assert!(set.insert(pk2));
    assert!(!set.insert(pk2));
    assert!(set.remove(pk2));
    assert!(!set.remove(pk2));

    assert!(set.insert(pk3));
    assert!(!set.insert(pk3));
}
