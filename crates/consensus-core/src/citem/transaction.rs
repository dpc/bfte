use bfte_util_array_type::array_type_define;
use bincode::{Decode, Encode};

use super::transaction_nonce::TransactionNonce;
use super::{IInput, IOutput, ModuleDyn};

#[derive(Encode, Decode)]
pub struct TransactionUnsigned {
    nonce: TransactionNonce,
    inputs: Vec<ModuleDyn<dyn IInput>>,
    outputs: Vec<ModuleDyn<dyn IOutput>>,
}

#[derive(Encode, Decode)]
pub struct Transaction {
    inner: TransactionUnsigned,
    signature: TransactionSignature,
}

array_type_define! {
    #[derive(Encode, Decode, Copy, Clone)]
    pub struct TransactionSignature[32];
}
