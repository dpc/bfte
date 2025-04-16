// SPDX-License-Identifier: MIT

use std::fmt;
use std::str::FromStr;

use hkdf::hmac::SimpleHmac;
use rand::Rng;
use snafu::Snafu;

#[derive(Clone, Copy, Debug)]
pub struct ChildId(u32);

impl ChildId {
    pub const fn new(v: u32) -> Self {
        Self(v)
    }
}

impl From<u32> for ChildId {
    fn from(value: u32) -> Self {
        ChildId(value)
    }
}

#[derive(Snafu, Debug)]
pub struct LevelError {
    required: u32,
    actual: u32,
}

pub type LevelResult<T> = Result<T, LevelError>;

#[derive(Clone, Copy)]
pub struct DeriveableSecret {
    bytes: [u8; 32],
    level: u32,
}

impl fmt::Debug for DeriveableSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeriveableSecret")
            .field("level", &self.level)
            .finish()
    }
}
impl DeriveableSecret {
    pub fn generate() -> Self {
        Self {
            bytes: rand::thread_rng().r#gen(),
            level: 0,
        }
    }

    pub fn is_root(self) -> bool {
        self.level == 0
    }

    pub fn ensure_level(self, level: u32) -> LevelResult<()> {
        if self.level != level {
            (LevelSnafu {
                required: level,
                actual: self.level,
            })
            .fail()?;
        }
        Ok(())
    }

    pub fn reveal_bytes(self) -> [u8; 32] {
        self.bytes
    }

    pub fn reveal_display(self) -> DisplayDeriveableSecret {
        DisplayDeriveableSecret(self)
    }

    pub fn derive(self, leaf: ChildId) -> Self {
        let v: hkdf::Hkdf<blake3::Hasher, SimpleHmac<blake3::Hasher>> =
            hkdf::Hkdf::<blake3::Hasher, SimpleHmac<_>>::new(Some("bfte".as_bytes()), &self.bytes);
        let mut bytes = [0; 32];
        v.expand(&leaf.0.to_be_bytes(), &mut bytes[..])
            .expect("Cant fail");
        Self {
            bytes,
            level: self.level.checked_add(1).expect("Can't overflow level"),
        }
    }
}

pub struct DisplayDeriveableSecret(DeriveableSecret);

impl fmt::Display for DisplayDeriveableSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            &bip39::Mnemonic::from_entropy(self.0.bytes.as_slice())
                .expect("Fixed len, can't fail")
                .to_string(),
        )
    }
}

#[derive(Debug, Snafu)]
#[snafu(display("SecretKey error: {msg}"))]
pub struct DeriveableSecretError {
    msg: String,
}
impl AsRef<str> for DeriveableSecretError {
    fn as_ref(&self) -> &str {
        self.msg.as_str()
    }
}

impl From<String> for DeriveableSecretError {
    fn from(msg: String) -> Self {
        Self { msg }
    }
}

impl FromStr for DeriveableSecret {
    type Err = DeriveableSecretError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = bip39::Mnemonic::from_str(s)
            .map_err(|e| format!("Mnemonic deserialization error: {e}"))?
            .to_entropy();
        if bytes.len() != 32 {
            return Err(("Invalid length").to_string().into());
        }
        Ok(Self {
            bytes: bytes.try_into().expect("Just checked length"),
            level: 0,
        })
    }
}

#[cfg(test)]
mod tests;
