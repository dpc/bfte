use std::fmt;
use std::io::Write as _;
use std::sync::Arc;

use bfte_util_array_type::array_type_define;
use bincode::{Decode, Encode};

use crate::signed::Hashable;

#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub struct ModuleParamsRaw(Arc<[u8]>);

impl From<Vec<u8>> for ModuleParamsRaw {
    fn from(value: Vec<u8>) -> Self {
        Self(value.into())
    }
}
impl From<Arc<[u8]>> for ModuleParamsRaw {
    fn from(value: Arc<[u8]>) -> Self {
        Self(value)
    }
}
impl fmt::Debug for ModuleParamsRaw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModuleConfigRaw")
            .field("len", &self.0.len())
            .finish_non_exhaustive()
    }
}

impl Hashable for ModuleParamsRaw {
    fn hash(&self) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new();

        hasher
            .write_all(&self.0)
            .expect("Can't fail to write to hasher");

        hasher.finalize()
    }
}

array_type_define! {
    pub struct ModuleConfigHash[32];
}
// framed_payload_define! {
//     /// Raw, undecoded module config
//     pub struct ModuleConfigRaw;

//     ModuleConfigHash;
//     ModuleConfigLen;

//     pub struct ModuleConfigSlice;

//     TAG = *b"moco";
// }
