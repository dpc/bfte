pub mod config;

use bincode::{Decode, Encode};
use derive_more::Display;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, Debug, Display)]
pub struct ModuleId(u32);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, Debug, Display)]
pub struct ModuleKind(u32);
