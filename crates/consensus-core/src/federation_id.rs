use bfte_util_array_type::{
    array_type_define, array_type_impl_base32_str, array_type_impl_debug_as_display,
    array_type_impl_serde, array_type_impl_zero_default,
};
use bincode::{Decode, Encode};

array_type_define! {
    #[derive(Encode, Decode, Copy, Clone)]
    pub struct FederationId[32];
}
array_type_impl_zero_default!(FederationId);
array_type_impl_base32_str!(FederationId);
array_type_impl_serde!(FederationId);
array_type_impl_debug_as_display!(FederationId);

impl From<blake3::Hash> for FederationId {
    fn from(value: blake3::Hash) -> Self {
        Self(*value.as_bytes())
    }
}

impl From<FederationId> for blake3::Hash {
    fn from(value: FederationId) -> Self {
        blake3::Hash::from_bytes(value.0)
    }
}
