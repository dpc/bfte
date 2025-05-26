// SPDX-License-Identifier: MIT

//! Core types used in BFTE consensus
//!
//! Focused on serialization/encoding, conversions, etc of core data formats
//! used across the project.
use ::bincode::{Decode, Encode};
use bfte_util_array_type::{
    array_type_define, array_type_impl_base32_str, array_type_impl_debug_as_display,
    array_type_impl_serde, array_type_impl_zero_default,
};

pub mod bincode;
pub mod block;
pub mod citem;
pub mod consensus_params;
pub mod federation_id;
pub mod msg;
pub mod num_peers;
pub mod peer;
pub mod peer_set;
pub mod signed;
pub mod timestamp;
pub mod ver;
pub mod vote;
pub mod module;

array_type_define! {
    #[derive(Encode, Decode, Copy, Clone)]
    pub struct Signature[64];
}
array_type_impl_zero_default!(Signature);
array_type_impl_base32_str!(Signature);
array_type_impl_serde!(Signature);
array_type_impl_debug_as_display!(Signature);

impl From<Signature> for ed25519_dalek::Signature {
    fn from(value: Signature) -> Self {
        ed25519_dalek::Signature::from_bytes(&value.0)
    }
}
impl From<ed25519_dalek::Signature> for Signature {
    fn from(value: ed25519_dalek::Signature) -> Self {
        Self(value.to_bytes())
    }
}
