use std::sync::Arc;

use bfte_consensus_core::module::ModuleKind;
use bfte_util_bincode::decode_whole;
use bincode::{Decode, Encode};
use derive_more::Deref;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct EffectId(u32);

impl EffectId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

pub trait EffectKind: Encode + Decode<()> {
    const MODULE_KIND: ModuleKind;
    const EFFECT_ID: EffectId;
}

pub trait EffectKindExt: EffectKind {
    fn encode(&self) -> CItemEffect {
        let encoded = bincode::encode_to_vec(self, bincode::config::standard())
            .expect("encoding should not fail");
        CItemEffect {
            effect_id: Self::EFFECT_ID,
            raw: encoded.into(),
        }
    }

    fn decode(effect: &CItemEffect) -> Result<Self, bincode::error::DecodeError> {
        if effect.effect_id != Self::EFFECT_ID {
            return Err(bincode::error::DecodeError::Other("effect ID mismatch"));
        }
        decode_whole(&effect.raw, bincode::config::standard())
    }
}

impl<T: EffectKind> EffectKindExt for T {}

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

    pub fn module_kind(&self) -> ModuleKind {
        self.module_kind
    }

    pub fn inner(&self) -> &CItemEffect {
        &self.inner
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
