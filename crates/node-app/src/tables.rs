use bfte_consensus_core::block::BlockRound;
use bfte_util_db::def_table;
use bincode::{Decode, Encode};
use derive_more::{Display, From, Into};

/// Position in the block of items that were already processed
#[derive(
    Encode, Decode, Default, PartialEq, Eq, PartialOrd, Ord, Into, From, Clone, Copy, Display,
)]
pub struct BlockCItemIdx(u32);

impl BlockCItemIdx {
    pub fn next(self) -> Self {
        Self(self.0.checked_add(1).expect("We can't overflow"))
    }
}

impl BlockCItemIdx {
    pub const fn new(val: u32) -> Self {
        Self(val)
    }
}

def_table! {
    /// As the `node-app` is processing citems from blocks
    /// it keeps track of its position here.
    app_cur_round: () => (BlockRound, BlockCItemIdx)
}
