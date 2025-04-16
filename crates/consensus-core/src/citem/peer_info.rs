// SPDX-License-Identifier: MIT

use bfte_util_array_type::{
    array_type_define, array_type_impl_base32_str, array_type_impl_debug_as_display,
    array_type_impl_serde, array_type_impl_zero_default,
};
use bincode::{Decode, Encode};

array_type_define! {
    #[derive(Encode, Decode, Clone)]
    pub struct IrohEndpoint[32];
}
array_type_impl_debug_as_display!(IrohEndpoint);
array_type_impl_zero_default!(IrohEndpoint);
array_type_impl_base32_str!(IrohEndpoint);
array_type_impl_serde!(IrohEndpoint);

#[derive(Encode, Decode, Clone)]
pub struct PeerInfo {
    name: String,
    iroh: IrohEndpoint,
}
