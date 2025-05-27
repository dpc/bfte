use bfte_util_array_type::array_type_fixed_size_define;
use bincode::{Decode, Encode};
use time::OffsetDateTime;

array_type_fixed_size_define! {
    /// Microsecond-precision absolute timestamp, UTC
    #[derive(Encode, Decode, Clone, Copy)]
    pub struct Timestamp(u64);
}

impl Timestamp {
    pub fn now() -> Self {
        Self::from(
            u64::try_from(OffsetDateTime::now_utc().unix_timestamp_nanos() / 1000)
                .expect("Can't fail"),
        )
    }
}
