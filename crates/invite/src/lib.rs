// SPDX-License-Identifier: MIT

use core::fmt;
use std::convert::Infallible;
use std::str::FromStr;

use bfte_consensus_core::block::{BlockHash, BlockRound};
use bfte_consensus_core::consensus_params::{ConsensusParamsHash, ConsensusParamsLen};
use bfte_node_core::address::PeerAddress;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt as _, Snafu};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Invite {
    /// Recent block "pin" to ensure the invitee joins the right federation
    #[serde(rename = "p")]
    pub pin: Option<(BlockRound, BlockHash)>,
    /// Hash and lan of initial consensus params to allow quickly joining
    /// consensus, especially if no blocks were created yet.
    #[serde(rename = "i")]
    pub init_params: Option<(ConsensusParamsHash, ConsensusParamsLen)>,

    /// Address of the peer to bootstrap from
    #[serde(rename = "a")]
    pub address: PeerAddress,
}

#[derive(Debug, Snafu)]
pub enum InviteParseError {
    Base32 {
        source: data_encoding::DecodeError,
    },
    Cbor {
        source: cbor4ii::serde::DecodeError<Infallible>,
    },
}

impl FromStr for Invite {
    type Err = InviteParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = data_encoding::BASE32_DNSCURVE
            .decode(s.as_bytes())
            .context(Base32Snafu)?;

        cbor4ii::serde::from_slice(&bytes).context(CborSnafu)
    }
}

impl fmt::Display for Invite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bytes = cbor4ii::serde::to_vec(vec![], &self).expect("Can't fail");
        f.write_fmt(format_args!(
            "{}",
            data_encoding::BASE32_DNSCURVE.encode_display(&bytes)
        ))
    }
}

pub struct InviteString(Invite);

impl From<InviteString> for Invite {
    fn from(value: InviteString) -> Self {
        value.0
    }
}

impl From<Invite> for InviteString {
    fn from(value: Invite) -> Self {
        Self(value)
    }
}

impl ::serde::Serialize for InviteString {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        s.serialize_str(&self.0.to_string())
    }
}

impl<'de> ::serde::de::Deserialize<'de> for InviteString {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        let str = <String>::deserialize(d)?;
        <Invite as std::str::FromStr>::from_str(&str)
            .map_err(|e| serde::de::Error::custom(format!("Deserialization error: {e:#}")))
            .map(Self)
    }
}
