use core::fmt;

use bincode::{Decode, Encode};
use derive_more::From;

use crate::peer::PeerIdx;

#[derive(Debug, Clone, Copy, From, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct NumPeers(u8);

impl fmt::Display for NumPeers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl NumPeers {
    /// Total number of peers
    pub fn total(self) -> usize {
        self.0.into()
    }

    /// Max number of faulty nodes
    pub fn max_faulty(self) -> usize {
        self.total().saturating_sub(1) / 3
    }

    /// Number of peers required to reach consensus
    pub fn threshold(self) -> usize {
        self.total() - self.max_faulty()
    }

    /// Iterator over given number of [`PeerIdx`]es
    pub fn peer_idx_iter(self) -> impl Iterator<Item = PeerIdx> {
        (0..self.0).map(PeerIdx::new)
    }
}

pub trait ToNumPeers {
    fn to_num_peers(&self) -> NumPeers;
}

impl<T> ToNumPeers for [T] {
    fn to_num_peers(&self) -> NumPeers {
        let num_peers: u8 = <usize as TryInto<u8>>::try_into(self.len())
            .expect("ToNumPeers used for Vec of size larger than u8");
        NumPeers::from(num_peers)
    }
}

impl<T> ToNumPeers for Vec<T> {
    fn to_num_peers(&self) -> NumPeers {
        self.as_slice().to_num_peers()
    }
}

#[test]
fn num_peers_sanity() {
    use convi::CastFrom;
    for (n, f, t) in [(1, 0, 1), (2, 0, 2), (3, 0, 3), (4, 1, 3)] {
        let num = NumPeers::from(n);
        assert_eq!(usize::cast_from(n), num.total());
        assert_eq!(f, num.max_faulty());
        assert_eq!(t, num.threshold());
    }
}
