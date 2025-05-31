pub mod config;

use bincode::{Decode, Encode};
use derive_more::{Display, From};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, Debug, Display, From)]
pub struct ModuleId(u32);

impl ModuleId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, Debug, Display, From)]
pub struct ModuleKind(u32);

impl ModuleKind {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}
