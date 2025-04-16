use bfte_consensus_core::peer::PeerIdx;
use bfte_util_array_type::{array_type_define, array_type_impl_zero_default};
use bincode::{Decode, Encode};
use convi::CastInto as _;
use derive_more::{Deref, DerefMut};

array_type_define! {
#[derive( Copy, Clone, Deref, DerefMut, Debug,  Encode, Decode)]
    pub struct VoteSet[32];
}
array_type_impl_zero_default!(VoteSet);

impl std::ops::BitOr for VoteSet {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        let mut res = self;

        for i in 0..self.0.len() {
            res.0[i] |= rhs.0[i]
        }

        res
    }
}

impl VoteSet {
    fn byte(&self, peer_idx: PeerIdx) -> &u8 {
        let bit = usize::from(peer_idx);
        &self.0[31 - (bit >> 3)]
    }

    fn byte_mut(&mut self, peer_idx: PeerIdx) -> &mut u8 {
        let bit = usize::from(peer_idx);
        &mut self.0[31 - (bit >> 3)]
    }

    fn bit(peer_idx: PeerIdx) -> u8 {
        let bit = usize::from(peer_idx);

        1 << (bit & 0x7)
    }

    pub fn insert(&mut self, peer_idx: PeerIdx) {
        *self.byte_mut(peer_idx) |= Self::bit(peer_idx);
    }

    pub fn remove(&mut self, peer_idx: PeerIdx) {
        *self.byte_mut(peer_idx) &= !Self::bit(peer_idx);
    }

    pub fn contains(&self, peer_idx: PeerIdx) -> bool {
        *self.byte(peer_idx) & Self::bit(peer_idx) != 0
    }

    pub fn len(self) -> usize {
        self.0
            .into_iter()
            .fold(0, |acc, b| acc + b.count_ones())
            .cast_into()
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
pub mod tests;
