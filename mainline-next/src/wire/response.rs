use serde::{Deserialize, Serialize};
use serde_bencode::value::Value;

use super::{ByteArray, ByteString, CompactAddress, CompactNodes, Id};

/// Response fields shared by BEP 5 and BEP 44. The query determines which are required.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ResponseArguments {
    pub id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<ByteString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<CompactNodes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<CompactAddress>>,
    #[serde(rename = "v", skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(rename = "k", skip_serializing_if = "Option::is_none")]
    pub key: Option<ByteArray<32>>,
    #[serde(rename = "sig", skip_serializing_if = "Option::is_none")]
    pub signature: Option<ByteArray<64>>,
    /// May be returned without `k`, `sig`, or `v` for a conditional GET.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
}
