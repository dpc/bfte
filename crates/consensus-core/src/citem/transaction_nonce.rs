use bfte_util_array_type::{
    array_type_define, array_type_impl_base32_str, array_type_impl_debug_as_display,
    array_type_impl_serde, array_type_impl_zero_default,
};
use bincode::{Decode, Encode};

array_type_define! {
    #[derive(Encode, Decode, Copy, Clone)]
    pub struct TransactionNonce[8];
}
array_type_impl_zero_default!(TransactionNonce);
array_type_impl_base32_str!(TransactionNonce);
array_type_impl_serde!(TransactionNonce);
array_type_impl_debug_as_display!(TransactionNonce);
