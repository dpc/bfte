use bincode::config;

pub const STANDARD_LIMIT_16M: usize = 0x1_0000_0000;
pub const STD_BINCODE_CONFIG: config::Configuration<
    config::BigEndian,
    config::Varint,
    config::Limit<STANDARD_LIMIT_16M>,
> = config::standard()
    .with_limit::<STANDARD_LIMIT_16M>()
    .with_big_endian()
    .with_variable_int_encoding();

/// Macro to generate types for handling signable/hashable payload types
/// representing an already encoded payload.
///
/// $payload is the main type, just the wrapper over `Arc<[u8]>`
/// with a corresponding $hash and $len types.
///
/// $slice, is a helper type that "encodes" the original
/// payload itself  - meaning it does not encode its length prefix, like
/// encoding Arc<[u8]> would normally do.
#[macro_export]
macro_rules! framed_payload_define {
    (
        $(#[$outer:meta])*
        $pv:vis struct $payload:tt;

        $hash:tt;
        $len:tt;

        $sv:vis struct $slice:tt;

        TAG = $tag:expr;
    ) => {

        $(#[$outer])*
        #[derive(PartialEq, Eq, Clone, Encode, Decode, Debug)]
        $pv struct $payload(std::sync::Arc<[u8]>);

        impl $payload {
            pub fn as_slice(&self) -> $slice {
                $slice(&self.0)
            }

            pub fn empty() -> Self {
                Self(Default::default())
            }

            pub fn hash(&self) -> $hash {
                 Hashable::hash(&self.as_slice()).into()
            }

            pub fn len(&self) -> $len {
                $len::from(u32::try_from(self.0.len()).expect("Can't fail"))
            }
        }

        impl From<$payload> for Arc<[u8]> {
            fn from(value: $payload) -> Self {
                value.0.clone()
            }
        }

        impl From<Arc<[u8]>> for $payload {
            fn from(value: Arc<[u8]>) -> Self {
                Self(value)
            }
        }

        impl From<Vec<u8>> for $payload {
            fn from(value: Vec<u8>) -> Self {
                Self(value.into())
            }
        }

        $(#[$outer])*
        #[derive(PartialEq, Eq)]
        $sv struct $slice<'b>(&'b [u8]);

        impl<'b> ::bincode::Encode for $slice<'b> {
            fn encode<E: ::bincode::enc::Encoder>(
                &self,
                encoder: &mut E,
            ) -> Result<(), ::bincode::error::EncodeError> {
                ::bincode::enc::write::Writer::write(encoder.writer(), &self.0)?;

                Ok(())
            }
        }

        impl<'b> ::std::ops::Deref for $slice<'b> {
            type Target = [u8];

            fn deref(&self) -> &[u8] {
                &self.0
            }
        }


        impl<'r> Hashable for $slice<'r> {
            const TAG: [u8; 4] = $tag;
        }

    }
}
