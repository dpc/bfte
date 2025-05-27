use core::fmt;

use bfte_util_array_type::array_type_fixed_size_define;
use bincode::{Decode, Encode};

array_type_fixed_size_define! {
    #[derive(Encode, Decode, Clone, Copy)]
    pub struct ConsensusVersionMajor(u16);
}

array_type_fixed_size_define! {
    #[derive(Encode, Decode, Clone, Copy)]
    pub struct ConsensusVersionMinor(u16);
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, Debug)]
pub struct ConsensusVersion {
    /// Immutable, major version the consensus started with
    ///
    /// Migrations between major versions are generally not supported,
    /// so once something (e.g. module) is running at given major consensus
    /// version, it is stuck there.
    major: ConsensusVersionMajor,

    /// Minor consensus version
    ///
    /// This version is expected to change as peers agree on it,
    /// and peer logic must support migrations.
    minor: ConsensusVersionMinor,
}

impl fmt::Display for ConsensusVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}.{}", self.major, self.minor))
    }
}

impl ConsensusVersion {
    pub const fn new_const(major: ConsensusVersionMajor, minor: ConsensusVersionMinor) -> Self {
        Self { major, minor }
    }

    pub fn new(
        major: impl Into<ConsensusVersionMajor>,
        minor: impl Into<ConsensusVersionMinor>,
    ) -> Self {
        Self {
            major: major.into(),
            minor: minor.into(),
        }
    }
}
