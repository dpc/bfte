use bfte_util_array_type::array_type_define;
use bincode::{Decode, Encode};

use super::transaction_nonce::TransactionNonce;
use super::{InputRaw, ModuleDyn, OutputRaw};

#[derive(Encode, Decode, Clone)]
pub struct TransactionUnsigned {
    pub nonce: TransactionNonce,
    pub inputs: Vec<ModuleDyn<InputRaw>>,
    pub outputs: Vec<ModuleDyn<OutputRaw>>,
}

#[derive(Encode, Decode, Clone)]
pub struct Transaction {
    pub inner: TransactionUnsigned,
    pub signature: TransactionSignature,
}

array_type_define! {
    #[derive(Encode, Decode, Copy, Clone)]
    pub struct TransactionSignature[32];
}
