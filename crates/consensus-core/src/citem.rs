// SPDX-License-Identifier: MIT

mod transaction_nonce;

use std::marker;
use std::sync::Arc;

use bincode::{BorrowDecode, Decode, Encode};
use transaction_nonce::TransactionNonce;

#[derive(Encode, Decode)]
pub struct ModuleId(u16);

/// Consensus item
///
/// Something that can transmitted and agreed on as a part of the consensus
/// process.
#[derive(Encode, Decode)]
pub enum Citem {
    Core(CoreCitem),
    Module(ModuleCitem),
    Transaction(Transaction),
}

#[derive(Encode, Decode)]
pub enum CoreCitem {}

#[derive(Encode, Decode)]
pub struct ModuleCitem {
    pub module_id: ModuleId,
}

#[derive(Encode, Decode)]
pub struct Transaction {
    nonce: TransactionNonce,
    inputs: Vec<Dyn<dyn IInput>>,
    outputs: Vec<Dyn<dyn IOutput>>,
}

pub struct Dyn<Iface: ?Sized> {
    module_id: ModuleId,
    inner: Arc<[u8]>,
    _marker: marker::PhantomData<Iface>,
}

impl<T, C> Decode<C> for Dyn<T>
where
    T: ?Sized,
{
    fn decode<D: bincode::de::Decoder<Context = C>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Ok(Self {
            module_id: Decode::decode(decoder)?,
            inner: Decode::decode(decoder)?,
            _marker: marker::PhantomData,
        })
    }
}
impl<'de, T, C> BorrowDecode<'de, C> for Dyn<T>
where
    T: ?Sized,
{
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de, Context = C>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Ok(Self {
            module_id: Decode::decode(decoder)?,
            inner: Decode::decode(decoder)?,
            _marker: marker::PhantomData,
        })
    }
}

impl<T> Encode for Dyn<T>
where
    T: ?Sized,
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
pub trait IInput {}
pub trait IOutput {}
