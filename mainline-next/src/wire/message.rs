use serde::{Deserialize, Serialize};

use super::{
    ByteString, CompactAddress, ErrorCode, Map, OptionalBool, ResponseArguments, WireQuery,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WireMessage {
    #[serde(rename = "t")]
    pub transaction_id: ByteString,
    #[serde(rename = "v", skip_serializing_if = "Option::is_none")]
    pub version: Option<ByteString>,
    #[serde(flatten)]
    pub kind: WireKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "y")]
pub(crate) enum WireKind {
    #[serde(rename = "q")]
    Query {
        #[serde(flatten)]
        query: WireQuery,
        #[serde(rename = "ro", default, skip_serializing_if = "OptionalBool::is_false")]
        read_only: OptionalBool,
    },
    #[serde(rename = "r")]
    Response {
        #[serde(rename = "r")]
        arguments: Map<ResponseArguments>,
        #[serde(rename = "ip", skip_serializing_if = "Option::is_none")]
        requester_address: Option<CompactAddress>,
    },
    #[serde(rename = "e")]
    Error {
        #[serde(rename = "e")]
        error: (ErrorCode, ByteString),
        #[serde(rename = "ip", skip_serializing_if = "Option::is_none")]
        requester_address: Option<CompactAddress>,
    },
}
