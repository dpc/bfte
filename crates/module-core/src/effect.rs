use std::sync::Arc;

use bfte_consensus_core::module::ModuleKind;
use bincode::{BorrowDecode, Decode, Encode};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct EffectId(u32);

pub trait EffectKind {
    const MODULE_KIND: ModuleKind;
    const EFFECT_ID: EffectId;

    type Payload: Encode + Decode<()>;
}

pub struct EffectDyn {
    module_kind: ModuleKind,
    effect_id: EffectId,
    inner: Arc<[u8]>,
}

impl<C> Decode<C> for EffectDyn {
    fn decode<D: bincode::de::Decoder<Context = C>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Ok(Self {
            module_kind: Decode::decode(decoder)?,
            effect_id: Decode::decode(decoder)?,
            inner: Decode::decode(decoder)?,
        })
    }
}

impl<'de, C> BorrowDecode<'de, C> for EffectDyn {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de, Context = C>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Ok(Self {
            module_kind: Decode::decode(decoder)?,
            effect_id: Decode::decode(decoder)?,
            inner: Decode::decode(decoder)?,
        })
    }
}

impl Encode for EffectDyn {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        self.module_kind.encode(encoder)?;
        self.effect_id.encode(encoder)?;
        self.inner.encode(encoder)?;
        Ok(())
    }
}
