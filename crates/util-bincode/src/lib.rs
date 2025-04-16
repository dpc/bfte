// SPDX-License-Identifier: MIT

pub use bincode;
use bincode::config::Config;
use bincode::{de, error};

pub fn decode_whole<D: de::Decode<()>, C: Config>(
    src: &[u8],
    config: C,
) -> Result<D, error::DecodeError> {
    let (t, consumed) = bincode::decode_from_slice(src, config)?;

    if consumed != src.len() {
        return Err(bincode::error::DecodeError::Other("leftover bytes"));
    }

    Ok(t)
}
