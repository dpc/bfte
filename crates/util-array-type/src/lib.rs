// SPDX-License-Identifier: MIT

pub use {data_encoding, rand, serde, serde_bytes};

#[macro_export]
macro_rules! array_type_define {
    (
        $(#[$outer:meta])*
        $v:vis struct $name:tt[$n:expr];
    ) => {

        $(#[$outer])*
        #[derive(PartialOrd, Ord, PartialEq, Eq)]
        $v struct $name([u8; $n]);

        impl $name {

            pub const LEN: usize = $n;
            pub const ZERO: Self = Self([0u8; $n]);
            pub const MIN: Self = Self([0u8; $n]);
            pub const MAX: Self = Self([0xffu8; $n]);

            pub fn as_slice(&self) -> &[u8] {
                self.0.as_slice()
            }

            pub fn from_bytes(bytes: [u8; $n]) -> Self {
                Self(bytes)
            }

            pub fn to_bytes(self) -> [u8; $n] {
                self.0
            }
        }
    }
}

#[macro_export]
macro_rules! array_type_impl_bytes_conv {
    ($name:tt) => {
        impl From<[u8; Self::LEN]> for $name {
            fn from(value: [u8; Self::LEN]) -> Self {
                Self(value)
            }
        }
        impl From<$name> for [u8; $name::LEN] {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

#[macro_export]
macro_rules! array_type_impl_zero_default {
    ($name:tt) => {
        impl Default for $name {
            fn default() -> Self {
                Self([0; Self::LEN])
            }
        }
    };
}

#[macro_export]
macro_rules! array_type_impl_debug_as_display {
    ($name:tt) => {
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                <Self as std::fmt::Display>::fmt(self, f)
            }
        }
    };
}

#[macro_export]
macro_rules! array_type_impl_serde {
    (
        $name:tt
    ) => {
        impl ::serde::Serialize for $name {
            fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
            where
                S: ::serde::Serializer,
            {
                if s.is_human_readable() {
                    s.serialize_str(&self.to_string())
                } else {
                    s.serialize_bytes(&self.0)
                }
            }
        }

        impl<'de> ::serde::de::Deserialize<'de> for $name {
            fn deserialize<D>(d: D) -> Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                if d.is_human_readable() {
                    let str = <String>::deserialize(d)?;
                    <Self as std::str::FromStr>::from_str(&str).map_err(|e| {
                        $crate::serde::de::Error::custom(format!("Deserialization error: {e:#}"))
                    })
                } else {
                    let bytes = <$crate::serde_bytes::ByteArray<{ $name::LEN }>>::deserialize(d)?;
                    Ok(Self(bytes.into_array()))
                }
            }
        }
    };
}

#[macro_export]
macro_rules! array_type_impl_base32_str {
    (
        $name:tt
    ) => {
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                $crate::data_encoding::BASE32_DNSCURVE.encode_write(self.as_slice(), f)
            }
        }

        impl std::str::FromStr for $name {
            type Err = $crate::data_encoding::DecodeError;

            fn from_str(s: &str) -> Result<$name, Self::Err> {
                let v = $crate::data_encoding::BASE32_DNSCURVE.decode(s.as_bytes())?;
                let a = v
                    .try_into()
                    .map_err(|_| $crate::data_encoding::DecodeError {
                        position: 0,
                        kind: $crate::data_encoding::DecodeKind::Length,
                    })?;
                Ok(Self(a))
            }
        }
    };
}

#[macro_export]
macro_rules! array_type_impl_base64_str {
    (
        $name:tt
    ) => {
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                data_encoding::BASE64_URL.encode_write(self.as_slice(), f)
            }
        }

        impl std::str::FromStr for $name {
            type Err = data_encoding::DecodeError;

            fn from_str(s: &str) -> Result<$name, Self::Err> {
                let v = data_encoding::BASE64_URL.decode(s.as_bytes())?;
                let a = v.try_into().map_err(|_| data_encoding::DecodeError {
                    position: 0,
                    kind: data_encoding::DecodeKind::Length,
                })?;
                Ok(Self(a))
            }
        }
    };
}

#[macro_export]
macro_rules! array_type_impl_rand {
    (
        $name:tt
    ) => {
        impl $crate::rand::distributions::Distribution<$name> for rand::distributions::Standard {
            fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> $name {
                $name(rng.r#gen())
            }
        }
    };
}

#[macro_export]
macro_rules! array_type_fixed_size_define {
    (
        $(#[$outer:meta])*
        $v:vis struct $name:ident($t:ty);
    ) => {

        $crate::array_type_define! {
            $(#[$outer])*
            $v struct $name[std::mem::size_of::<$t>()];
        }
        $crate::array_type_impl_debug_as_display!($name);
        $crate::array_type_impl_zero_default!($name);

        impl $name {
            pub const fn new(t: $t) -> Self {
                Self(t.to_be_bytes())
            }

            pub const fn to_number(self) -> $t {
                <$t>::from_be_bytes(self.0)
            }

            pub const fn from_number(t: $t) -> Self {
                Self(t.to_be_bytes())
            }
        }


        impl From<$t> for $name {
            fn from(value: $t) -> Self {
                Self(value.to_be_bytes())
            }
        }

        impl From<$name> for $t {
            fn from(value: $name) -> Self {
                <$t>::from_be_bytes(value.0)
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_fmt(format_args!("{}", <$t>::from(*self)))
            }
        }

        impl $name {
            pub fn next(self) -> Option<Self> {
                <$t>::from(self).checked_add(1).map(Self::from)
            }
            pub fn next_expect(self) -> Self {
                Self::from(<$t>::from(self).checked_add(1).expect("Can't run out of u64 rounds"))
            }
            pub fn next_wrapping(self) -> Self {
                Self::from(<$t>::from(self).wrapping_add(1))
            }
            pub fn prev(self) -> Option<Self> {
                <$t>::from(self).checked_sub(1).map(Self::from)
            }
        }

        impl $name {
            pub fn checked_add(self, rhs: $t) -> Option<Self> {
                <$t>::from(self).checked_add(rhs).map(Self::from)
            }
        }

    };
}

#[macro_export]
macro_rules! array_type_fixed_size_impl_serde {
    (
        $name:tt
    ) => {
        impl $crate::serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: $crate::serde::Serializer,
            {
                self.to_number().serialize(serializer)
            }
        }

        impl<'de> $crate::serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: $crate::serde::Deserializer<'de>,
            {
                Ok(Self::from_number(Deserialize::deserialize(deserializer)?))
            }
        }
    };
}
