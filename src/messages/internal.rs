use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DHTMessage {
    #[serde(rename = "t", with = "serde_bytes")]
    pub transaction_id: Vec<u8>,

    #[serde(default)]
    #[serde(rename = "v", with = "serde_bytes")]
    pub version: Option<Vec<u8>>,

    #[serde(flatten)]
    pub variant: DHTMessageVariant,

    #[serde(default)]
    #[serde(with = "serde_bytes")]
    pub ip: Option<Vec<u8>>,

    #[serde(default)]
    #[serde(rename = "ro")]
    pub read_only: Option<i32>,
}

impl DHTMessage {
    pub fn from_bytes<T: AsRef<[u8]>>(bytes: T) -> Result<DHTMessage> {
        let bytes = bytes.as_ref();
        let obj = serde_bencode::from_bytes(bytes)?;
        Ok(obj)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        serde_bencode::to_bytes(self).map_err(Error::BencodeError)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "y")]
pub enum DHTMessageVariant {
    #[serde(rename = "q")]
    Request(DHTRequestSpecific),

    #[serde(rename = "r")]
    Response(DHTResponseSpecific),

    #[serde(rename = "e")]
    Error(DHTErrorSpecific),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "q")]
pub enum DHTRequestSpecific {
    #[serde(rename = "ping")]
    Ping {
        #[serde(rename = "a")]
        arguments: DHTPingArguments,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)] // This means order matters! Order these from most to least detailed
pub enum DHTResponseSpecific {
    Ping {
        #[serde(rename = "r")]
        arguments: DHTPingResponseArguments,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DHTErrorSpecific {
    #[serde(rename = "e")]
    pub error_info: Vec<serde_bencode::value::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum DHTErrorValue {
    #[serde(rename = "")]
    ErrorCode(i32),
    ErrorDescription(String),
}

// === PING ===

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DHTPingArguments {
    #[serde(with = "serde_bytes")]
    pub id: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DHTPingResponseArguments {
    #[serde(with = "serde_bytes")]
    pub id: Vec<u8>,
}
