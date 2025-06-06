// SPDX-License-Identifier: MIT

pub mod transaction;
pub mod transaction_nonce;
use std::sync::Arc;

use bincode::{BorrowDecode, Decode, Encode};
use derive_more::Deref;
use transaction::Transaction;

use crate::module::ModuleId;
use crate::peer::PeerPubkey;

/// Consensus item
///
/// Something that can transmitted and agreed on as a part of the consensus
/// process.
#[derive(Encode, Decode)]
pub enum CItem {
    /// Consensus item proposed by the peer being the round leader for a block
    /// round.
    PeerCItem(ModuleDyn<CItemRaw>),
    /// A signed transaction aggregating inputs and outputs.
    Transaction(Transaction),
}

#[derive(Encode, Decode)]
pub enum CoreCitem {
    AddPeerVote(PeerPubkey),
    RemovePeerVote(PeerPubkey),
}

#[derive(Deref, Encode, Decode, Clone)]
pub struct InputRaw(pub Arc<[u8]>);

#[derive(Deref, Encode, Decode, Clone)]
pub struct OutputRaw(pub Arc<[u8]>);

#[derive(Deref, Encode, Decode, Clone)]
pub struct CItemRaw(pub Arc<[u8]>);

#[derive(PartialEq, Eq, Clone)]
pub struct ModuleDyn<Inner> {
    module_id: ModuleId,
    inner: Inner,
}

impl<Inner> ModuleDyn<Inner> {
    pub fn new(module_id: ModuleId, inner: Inner) -> Self {
        Self { module_id, inner }
    }

    pub fn module_id(&self) -> ModuleId {
        self.module_id
    }

    pub fn inner(&self) -> &Inner {
        &self.inner
    }
}
impl<Inner, C> Decode<C> for ModuleDyn<Inner>
where
    Inner: Decode<C>,
{
    fn decode<D: bincode::de::Decoder<Context = C>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Ok(Self {
            module_id: Decode::decode(decoder)?,
            inner: Decode::decode(decoder)?,
        })
    }
}

impl<'de, Inner, C> BorrowDecode<'de, C> for ModuleDyn<Inner>
where
    Inner: Decode<C>,
{
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de, Context = C>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Ok(Self {
            module_id: Decode::decode(decoder)?,
            inner: Decode::decode(decoder)?,
        })
    }
}

impl<Inner> Encode for ModuleDyn<Inner>
where
    Inner: Encode,
{
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        self.module_id.encode(encoder)?;
        self.inner.encode(encoder)?;
        Ok(())
    }
}
