use std::collections::BTreeMap;
use std::io::Write as _;
use std::{ops, result};

use bincode::{Decode, Encode};
use ed25519_dalek::ed25519::signature::SignerMut as _;
use snafu::{OptionExt as _, Snafu};

use crate::Signature;
use crate::bincode::CONSENSUS_BINCODE_CONFIG;
use crate::consensus_params::ConsensusParams;
use crate::peer::{PeerIdx, PeerPubkey, PeerSeckey};

#[derive(Debug, Snafu)]
pub struct InvalidSignatureError;

pub type InvalidSignatureResult<T> = Result<T, InvalidSignatureError>;

pub trait Hashable: bincode::Encode {
    fn hash(&self) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new();

        bincode::encode_into_std_write(self, &mut hasher, CONSENSUS_BINCODE_CONFIG)
            .expect("Can't fail");

        hasher.finalize()
    }
}

/// A message that can be signed/verified by [`PeerPubkey`] identity
pub trait Signable: Hashable {
    /// Unique tag preventing two different type of messages with the same
    /// encoding from conflicting with each other
    const TAG: [u8; 4];

    fn sign_hash(&self) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new();

        hasher.write_all(b"bfte").expect("Can't fail");
        hasher.write_all(&Self::TAG).expect("Can't fail");
        hasher
            .write_all(self.hash().as_bytes())
            .expect("Can't fail");

        hasher.finalize()
    }

    fn sign_with(&self, seckey: PeerSeckey) -> Signature {
        let v = ed25519_dalek::SigningKey::from(seckey).sign(self.sign_hash().as_bytes());
        v.into()
    }

    fn verify_signature(&self, pubkey: PeerPubkey, sig: Signature) -> InvalidSignatureResult<()> {
        verify_hash_signature(self.sign_hash(), pubkey, sig)
    }
}

fn verify_hash_signature(
    hash: blake3::Hash,
    pubkey: PeerPubkey,
    sig: Signature,
) -> InvalidSignatureResult<()> {
    ed25519_dalek::VerifyingKey::try_from(pubkey)
        .expect("At this point pubkeys must be valid")
        .verify_strict(hash.as_bytes(), &sig.into())
        .ok()
        .context(InvalidSignatureSnafu)?;
    Ok(())
}

#[derive(Decode, Encode, Clone, Copy, Debug)]
pub struct Signed<T> {
    pub inner: T,
    pub sig: Signature,
}

impl<T> Signed<T>
where
    T: Signable,
{
    pub fn new(inner: T, sig: Signature) -> Self {
        Self { inner, sig }
    }

    pub fn new_sign(inner: T, seckey: PeerSeckey) -> Self {
        let sig = inner.sign_with(seckey);
        Self { inner, sig }
    }

    pub fn verify_sig_peer_idx(
        &self,
        peer_idx: PeerIdx,
        peer_keys: &[PeerPubkey],
    ) -> result::Result<(), InvalidSignatureError> {
        self.verify_signature(peer_keys[peer_idx.as_usize()], self.sig)
    }

    pub fn verify_sig_peer_pubkey(
        &self,
        peer_pubkey: PeerPubkey,
    ) -> result::Result<(), InvalidSignatureError> {
        self.verify_signature(peer_pubkey, self.sig)
    }
}

impl<T> ops::Deref for Signed<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Decode, Encode, Clone, Debug)]
pub struct Notarized<T> {
    pub inner: T,
    pub sigs: BTreeMap<PeerIdx, Signature>,
}

impl<T> Notarized<T> {
    pub fn new(inner: T, sigs: impl IntoIterator<Item = (PeerIdx, Signature)>) -> Self {
        Self {
            inner,
            sigs: sigs.into_iter().collect(),
        }
    }
}
impl<T> ops::Deref for Notarized<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Debug, Snafu, PartialEq, Eq)]
pub enum InvalidNotarizationError {
    NotEnoughSignatures,
    InvalidPeerSignature { peer_idx: PeerIdx },
}

impl<T> Notarized<T>
where
    T: Signable,
{
    pub fn verify_sigs(
        &self,
        consensus_params: &ConsensusParams,
    ) -> result::Result<(), InvalidNotarizationError> {
        if self.sigs.len() < consensus_params.num_peers().threshold() {
            NotEnoughSignaturesSnafu.fail()?;
        }

        let hash = self.sign_hash();

        for (peer_idx, sig) in &self.sigs {
            verify_hash_signature(hash, consensus_params.peers[peer_idx.as_usize()], *sig)
                .ok()
                .context(InvalidPeerSignatureSnafu {
                    peer_idx: *peer_idx,
                })?;
        }
        Ok(())
    }
}
