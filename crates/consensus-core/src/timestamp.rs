use bfte_util_array_type::array_type_fixed_size_define;
use bincode::{Decode, Encode};
use time::UtcDateTime;

array_type_fixed_size_define! {
    /// Microsecond-precision absolute timestamp, UTC
    #[derive(Encode, Decode, Clone, Copy)]
    pub struct Timestamp(u64);
}

impl Timestamp {
    pub fn now() -> Self {
        Self::from(
            u64::try_from(UtcDateTime::now().unix_timestamp_nanos() / 1000).expect("Can't fail"),
        )
    }

    /// Convert to datetime, if in range
    pub fn to_datetime(self) -> Option<UtcDateTime> {
        UtcDateTime::from_unix_timestamp_nanos(i128::from(self.to_number() * 1000)).ok()
    }
}
