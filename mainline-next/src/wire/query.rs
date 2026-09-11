//! Command-specific query arguments, without domain conversions or item policy.

use serde::{Deserialize, Serialize};
use serde_bencode::value::Value;

use super::{ByteArray, ByteString, Id, Map, OptionalBool};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "q", rename_all = "snake_case")]
pub(crate) enum WireQuery {
    Ping {
        #[serde(rename = "a")]
        arguments: Map<PingArguments>,
    },
    FindNode {
        #[serde(rename = "a")]
        arguments: Map<FindNodeArguments>,
    },
    GetPeers {
        #[serde(rename = "a")]
        arguments: Map<GetPeersArguments>,
    },
    AnnouncePeer {
        #[serde(rename = "a")]
        arguments: Map<AnnouncePeerArguments>,
    },
    Get {
        #[serde(rename = "a")]
        arguments: Map<GetArguments>,
    },
    Put {
        #[serde(rename = "a")]
        arguments: Map<PutArguments>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PingArguments {
    pub id: Id,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FindNodeArguments {
    pub id: Id,
    pub target: Id,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GetPeersArguments {
    pub id: Id,
    pub info_hash: Id,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AnnouncePeerArguments {
    pub id: Id,
    pub info_hash: Id,
    pub port: u16,
    #[serde(default, skip_serializing_if = "OptionalBool::is_false")]
    pub implied_port: OptionalBool,
    pub token: ByteString,
}

/// BEP 44 lookup for either an immutable or mutable item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GetArguments {
    pub id: Id,
    pub target: Id,
    /// Request the mutable value only if its sequence is greater than this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
}

/// BEP 44 store arguments. Mutable-field consistency is checked separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PutArguments {
    pub id: Id,
    pub token: ByteString,
    #[serde(rename = "v")]
    pub value: Value,
    #[serde(rename = "k", skip_serializing_if = "Option::is_none")]
    pub key: Option<ByteArray<32>>,
    #[serde(rename = "sig", skip_serializing_if = "Option::is_none")]
    pub signature: Option<ByteArray<64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub salt: Option<ByteString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cas: Option<i64>,
}
