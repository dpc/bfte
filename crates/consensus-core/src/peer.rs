use core::fmt;

use bfte_util_array_type::{
    array_type_define, array_type_impl_base32_str, array_type_impl_bytes_conv,
    array_type_impl_debug_as_display, array_type_impl_rand, array_type_impl_serde,
    array_type_impl_zero_default,
};
use bincode::{Decode, Encode};
use convi::CastInto as _;
use derive_more::From;
use snafu::Snafu;

/// Peer index
///
/// The set of peers is known within the consensus,
/// so might refer to them by index, to save space.
#[derive(Encode, Decode, From, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct PeerIdx(u8);

impl fmt::Display for PeerIdx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", self.0))
    }
}
impl PeerIdx {
    pub const MIN: Self = PeerIdx(0x00);
    pub const MAX: Self = PeerIdx(0xff);

    pub const fn new(i: u8) -> Self {
        Self(i)
    }

    pub fn as_usize(self) -> usize {
        self.0.cast_into()
    }
}

impl From<PeerIdx> for usize {
    fn from(value: PeerIdx) -> Self {
        usize::from(value.0)
    }
}

array_type_define! {
    #[derive(Encode, Decode, Clone, Copy, Hash)]
    pub struct PeerPubkey[32];
}

impl PeerPubkey {
    pub fn to_short(self) -> PeerPubkeyShort {
        PeerPubkeyShort(self)
    }
}

pub struct PeerPubkeyShort(PeerPubkey);

impl fmt::Display for PeerPubkeyShort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{}...{}",
            data_encoding::BASE32_DNSCURVE.encode_display(&self.0.as_slice()[0..4]),
            data_encoding::BASE32_DNSCURVE.encode_display(&self.0.as_slice()[28..32])
        ))
    }
}

array_type_impl_zero_default!(PeerPubkey);
array_type_impl_base32_str!(PeerPubkey);
array_type_impl_serde!(PeerPubkey);
array_type_impl_debug_as_display!(PeerPubkey);
array_type_impl_rand!(PeerPubkey);

#[derive(Debug, Snafu)]
pub struct InvalidPubkeyError;

impl TryFrom<PeerPubkey> for ed25519_dalek::VerifyingKey {
    type Error = InvalidPubkeyError;

    fn try_from(value: PeerPubkey) -> Result<Self, Self::Error> {
        ed25519_dalek::VerifyingKey::from_bytes(&value.0).map_err(|_| InvalidPubkeyError)
    }
}

array_type_define! {
    #[derive(Encode, Decode, Clone, Copy)]
    pub struct PeerSeckey[32];
}

impl PeerSeckey {
    pub fn generate() -> Self {
        Self(ed25519_dalek::SigningKey::generate(&mut rand::thread_rng()).to_bytes())
    }

    pub fn pubkey(self) -> PeerPubkey {
        PeerPubkey(
            ed25519_dalek::SigningKey::from(self)
                .verifying_key()
                .to_bytes(),
        )
    }
}

impl From<PeerSeckey> for ed25519_dalek::SigningKey {
    fn from(value: PeerSeckey) -> Self {
        ed25519_dalek::SigningKey::from_bytes(&value.0)
    }
}

array_type_impl_bytes_conv!(PeerSeckey);
array_type_impl_zero_default!(PeerSeckey);
