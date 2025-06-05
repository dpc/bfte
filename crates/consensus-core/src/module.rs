pub mod config;

use bincode::{Decode, Encode};
use derive_more::{Display, From};
use serde::{Deserialize, Serialize};

#[derive(
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Encode,
    Decode,
    Debug,
    Display,
    From,
    Hash,
    Serialize,
    Deserialize,
)]
pub struct ModuleId(u32);

impl ModuleId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

#[derive(
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Encode,
    Decode,
    Debug,
    Display,
    From,
    Hash,
    Serialize,
    Deserialize,
)]
pub struct ModuleKind(u32);

impl ModuleKind {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}
