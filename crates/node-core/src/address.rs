use bfte_util_array_type::{
    array_type_define, array_type_impl_base32_str, array_type_impl_debug_as_display,
    array_type_impl_serde, array_type_impl_zero_default,
};
use bfte_util_error::Whatever;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use snafu::ResultExt as _;

array_type_define! {
    #[derive(Encode, Decode, Copy, Clone)]
    pub struct IrohAddress[32];
}
array_type_impl_zero_default!(IrohAddress);
array_type_impl_base32_str!(IrohAddress);
array_type_impl_serde!(IrohAddress);
array_type_impl_debug_as_display!(IrohAddress);

impl From<iroh_base::NodeId> for IrohAddress {
    fn from(value: iroh_base::NodeId) -> Self {
        Self(*value.as_bytes())
    }
}

impl TryFrom<IrohAddress> for iroh_base::NodeId {
    type Error = Whatever;

    fn try_from(value: IrohAddress) -> Result<Self, Self::Error> {
        iroh_base::NodeId::from_bytes(&value.to_bytes()).whatever_context("Invalid iroh nodeId")
    }
}

#[derive(Clone, Debug, Encode, Decode, Serialize, Deserialize, Copy, PartialEq, Eq)]
pub enum PeerAddress {
    Iroh(IrohAddress),
}
