use std::ops;

use bincode::{Decode, Encode};

use crate::num_peers::{NumPeers, ToNumPeers};
use crate::peer::PeerPubkey;

#[derive(Debug, Clone, Encode, Decode, Default, PartialEq, Eq)]
pub struct PeerSet(Vec<PeerPubkey>);

impl ops::Deref for PeerSet {
    type Target = [PeerPubkey];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PeerSet {
    pub fn new() -> Self {
        Self(vec![])
    }

    pub fn as_slice(&self) -> &[PeerPubkey] {
        &self.0
    }

    pub fn insert(&mut self, peer_pubkey: PeerPubkey) -> bool {
        if self.0.binary_search(&peer_pubkey).is_ok() {
            return false;
        }
        self.0.push(peer_pubkey);
        self.0.sort_unstable();
        true
    }

    pub fn remove(&mut self, peer_pubkey: PeerPubkey) -> bool {
        if let Ok(index) = self.0.binary_search(&peer_pubkey) {
            self.0.swap_remove(index);

            self.0.sort_unstable();
            true
        } else {
            false
        }
    }
}

impl ToNumPeers for PeerSet {
    fn to_num_peers(&self) -> NumPeers {
        self.0.to_num_peers()
    }
}

impl FromIterator<PeerPubkey> for PeerSet {
    fn from_iter<T: IntoIterator<Item = PeerPubkey>>(iter: T) -> Self {
        let mut items = Vec::from_iter(iter);
        items.sort_unstable();
        Self(items)
    }
}

impl IntoIterator for PeerSet {
    type Item = PeerPubkey;

    type IntoIter = <Vec<PeerPubkey> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a PeerSet {
    type Item = &'a PeerPubkey;

    type IntoIter = <&'a [PeerPubkey] as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.as_slice().iter()
    }
}

impl<const N: usize> From<[PeerPubkey; N]> for PeerSet {
    fn from(mut value: [PeerPubkey; N]) -> Self {
        value.sort_unstable();
        Self(value.to_vec())
    }
}
impl From<Vec<PeerPubkey>> for PeerSet {
    fn from(mut value: Vec<PeerPubkey>) -> Self {
        value.sort_unstable();
        Self(value)
    }
}

#[cfg(test)]
mod tests;
