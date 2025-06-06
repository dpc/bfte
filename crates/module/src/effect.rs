use std::sync::Arc;

use bfte_consensus_core::module::ModuleKind;
use bincode::{Decode, Encode};
use derive_more::Deref;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct EffectId(u32);

impl EffectId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

pub trait EffectKind {
    const MODULE_KIND: ModuleKind;
    const EFFECT_ID: EffectId;

    type Payload: Encode + Decode<()>;
}

#[derive(Deref, Encode, Decode)]
pub struct CItemEffect {
    pub effect_id: EffectId,
    #[deref]
    pub raw: Arc<[u8]>,
}

#[derive(Encode, Decode)]
pub struct ModuleCItemEffect {
    module_kind: ModuleKind,
    inner: CItemEffect,
}

impl ModuleCItemEffect {
    pub fn new(module_kind: ModuleKind, inner: CItemEffect) -> Self {
        Self { module_kind, inner }
    }
}

// impl<C> Decode<C> for ModuleCItemEffect {
//     fn decode<D: bincode::de::Decoder<Context = C>>(
//         decoder: &mut D,
//     ) -> Result<Self, bincode::error::DecodeError> {
//         Ok(Self {
//             module_kind: Decode::decode(decoder)?,
//             effect_id: Decode::decode(decoder)?,
//             inner: Decode::decode(decoder)?,
//         })
//     }
// }

// impl<'de, C> BorrowDecode<'de, C> for ModuleCItemEffect {
//     fn borrow_decode<D: bincode::de::BorrowDecoder<'de, Context = C>>(
//         decoder: &mut D,
//     ) -> Result<Self, bincode::error::DecodeError> {
//         Ok(Self {
//             module_kind: Decode::decode(decoder)?,
//             effect_id: Decode::decode(decoder)?,
//             inner: Decode::decode(decoder)?,
//         })
//     }
// }

// impl Encode for ModuleCItemEffect {
//     fn encode<E: bincode::enc::Encoder>(
//         &self,
//         encoder: &mut E,
//     ) -> Result<(), bincode::error::EncodeError> {
//         self.module_kind.encode(encoder)?;
//         self.effect_id.encode(encoder)?;
//         self.inner.encode(encoder)?;
//         Ok(())
//     }
// }
