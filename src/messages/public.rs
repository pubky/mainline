use super::internal;
use std::convert::TryInto;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use crate::common::{Id, Node, ID_SIZE};
use crate::{Error, Result};

#[derive(Debug, PartialEq, Clone)]
pub struct Message {
    pub transaction_id: Vec<u8>,

    /// The version of the requester or responder.
    pub version: Option<Vec<u8>>,

    /// The IP address and port ("SocketAddr") of the requester as seen from the responder's point of view.
    /// This should be set only on response, but is defined at this level with the other common fields to avoid defining yet another layer on the response objects.
    pub requester_ip: Option<SocketAddr>,

    pub message_type: MessageType,

    /// For bep0043. When set true on a request, indicates that the requester can't reply to requests and that responders should not add requester to their routing tables.
    /// Should only be set on requests - undefined behavior when set on a response.
    pub read_only: Option<bool>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum MessageType {
    Request(RequestSpecific),

    Response(ResponseSpecific),

    Error(ErrorSpecific),
}

#[derive(Debug, PartialEq, Clone)]
pub struct ErrorSpecific {
    pub code: i32,
    pub description: String,
}

#[derive(Debug, PartialEq, Clone)]
pub enum RequestSpecific {
    PingRequest(PingRequestArguments),
}

#[derive(Debug, PartialEq, Clone)]
pub enum ResponseSpecific {
    PingResponse(PingResponseArguments),
}

// === PING ===
#[derive(Debug, PartialEq, Clone)]
pub struct PingRequestArguments {
    pub requester_id: Id,
}

#[derive(Debug, PartialEq, Clone)]
pub struct PingResponseArguments {
    pub responder_id: Id,
}

impl Message {
    fn into_serde_message(self) -> internal::DHTMessage {
        internal::DHTMessage {
            transaction_id: self.transaction_id,
            version: self.version,
            ip: self
                .requester_ip
                .map(|sockaddr| sockaddr_to_bytes(&sockaddr)),
            read_only: self
                .read_only
                .map(|read_only| if read_only { 1 } else { 0 }),
            variant: match self.message_type {
                MessageType::Request(req) => internal::DHTMessageVariant::DHTRequest(match req {
                    RequestSpecific::PingRequest(ping_args) => internal::DHTRequestSpecific::Ping {
                        arguments: internal::DHTPingArguments {
                            id: ping_args.requester_id.to_vec(),
                        },
                    },
                }),

                MessageType::Response(res) => internal::DHTMessageVariant::DHTResponse(match res {
                    ResponseSpecific::PingResponse(ping_args) => {
                        internal::DHTResponseSpecific::Ping {
                            arguments: internal::DHTPingResponseArguments {
                                id: ping_args.responder_id.to_vec(),
                            },
                        }
                    }
                }),

                MessageType::Error(err) => {
                    internal::DHTMessageVariant::DHTError(internal::DHTErrorSpecific {
                        error_info: vec![
                            serde_bencode::value::Value::Int(err.code.into()),
                            serde_bencode::value::Value::Bytes(err.description.into()),
                        ],
                    })
                }
            },
        }
    }

    fn from_serde_message(msg: internal::DHTMessage) -> Result<Message> {
        Ok(Message {
            transaction_id: msg.transaction_id,
            version: msg.version,
            requester_ip: match msg.ip {
                Some(ip) => Some(bytes_to_sockaddr(ip)?),
                _ => None,
            },
            read_only: msg.read_only.map(|read_only| read_only >= 1),

            message_type: match msg.variant {
                internal::DHTMessageVariant::DHTRequest(req_variant) => {
                    MessageType::Request(match req_variant {
                        internal::DHTRequestSpecific::Ping { arguments } => {
                            RequestSpecific::PingRequest(PingRequestArguments {
                                requester_id: Id::from_bytes(&arguments.id)?,
                            })
                        }
                    })
                }

                internal::DHTMessageVariant::DHTResponse(res_variant) => {
                    MessageType::Response(match res_variant {
                        internal::DHTResponseSpecific::Ping { arguments } => {
                            ResponseSpecific::PingResponse(PingResponseArguments {
                                responder_id: Id::from_bytes(&arguments.id)?,
                            })
                        }
                    })
                }

                internal::DHTMessageVariant::DHTError(err) => {
                    if err.error_info.len() < 2 {
                        return Err(Error::Static(
                            "Error packet should have at least 2 elements",
                        ));
                    }
                    MessageType::Error(ErrorSpecific {
                        code: match err.error_info[0] {
                            serde_bencode::value::Value::Int(code) => match code.try_into() {
                                Ok(code) => code,
                                Err(e) => return Err(Error::Static("error parsing error code")),
                            },
                            _ => return Err(Error::Static("Expected error code as first element")),
                        },
                        description: match &err.error_info[1] {
                            serde_bencode::value::Value::Bytes(desc) => {
                                match std::str::from_utf8(desc) {
                                    Ok(desc) => desc.to_string(),
                                    Err(e) => {
                                        return Err(Error::Static(
                                            "error parsing error description",
                                        ))
                                    }
                                }
                            }
                            _ => {
                                return Err(Error::Static("Expected description as second element"))
                            }
                        },
                    })
                }
            },
        })
    }

    pub fn to_bytes(self) -> Result<Vec<u8>> {
        self.into_serde_message().to_bytes()
    }

    pub fn from_bytes<T: AsRef<[u8]>>(bytes: T) -> Result<Message> {
        Message::from_serde_message(internal::DHTMessage::from_bytes(bytes)?)
    }

    /// Return the Id of the sender of the Message
    ///
    /// This is less straightforward than it seems because not *all* messages are sent
    /// with an Id (all are except Error messages). This is reflected in the structure
    /// of DHT Messages, and makes it a bit annoying to learn the sender's Id without
    /// unraveling the entire message. This method is a convenience method to extract
    /// the sender (or "author") Id from the guts of any Message.
    pub fn get_author_id(&self) -> Option<Id> {
        let id = match &self.message_type {
            MessageType::Request(request_variant) => match request_variant {
                RequestSpecific::PingRequest(arguments) => arguments.requester_id,
            },
            MessageType::Response(response_variant) => match response_variant {
                ResponseSpecific::PingResponse(arguments) => arguments.responder_id,
            },
            MessageType::Error(_) => {
                return None;
            }
        };

        Some(id)
    }
}

fn bytes_to_sockaddr<T: AsRef<[u8]>>(bytes: T) -> Result<SocketAddr> {
    let bytes = bytes.as_ref();
    match bytes.len() {
        6 => {
            let ip = Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]);

            let port_bytes_as_array: [u8; 2] = bytes[4..6]
                .try_into()
                .map_err(|_| Error::Static("wrong number of bytes for port"))?;

            let port: u16 = u16::from_be_bytes(port_bytes_as_array);

            Ok(SocketAddr::new(IpAddr::V4(ip), port))
        }

        18 => Err(Error::Static("IPv6 is not yet implemented")),

        _ => Err(Error::Static("Wrong number of bytes for sockaddr")),
    }
}

pub fn sockaddr_to_bytes(sockaddr: &SocketAddr) -> Vec<u8> {
    let mut bytes = Vec::new();

    match sockaddr {
        SocketAddr::V4(v4) => {
            let ip_bytes = v4.ip().octets();
            for item in ip_bytes {
                bytes.push(item);
            }
        }

        SocketAddr::V6(v6) => {
            let ip_bytes = v6.ip().octets();
            for item in ip_bytes {
                bytes.push(item);
            }
        }
    }

    let port_bytes = sockaddr.port().to_be_bytes();
    bytes.push(port_bytes[0]);
    bytes.push(port_bytes[1]);

    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::prelude::*;

    #[test]
    fn test_ping_request() {
        let original_msg = Message {
            transaction_id: vec![0, 1, 2],
            version: None,
            requester_ip: None,
            read_only: None,
            message_type: MessageType::Request(RequestSpecific::PingRequest(
                PingRequestArguments {
                    requester_id: Id::random(),
                },
            )),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(bytes).unwrap();
        let parsed_msg = Message::from_serde_message(parsed_serde_msg).unwrap();
        assert_eq!(parsed_msg, original_msg);
    }

    #[test]
    fn test_ping_response() {
        let original_msg = Message {
            transaction_id: vec![1, 2, 3],
            version: Some(vec![0xde, 0xad]),
            requester_ip: Some("99.100.101.102:1030".parse().unwrap()),
            read_only: None,
            message_type: MessageType::Response(ResponseSpecific::PingResponse(
                PingResponseArguments {
                    responder_id: Id::random(),
                },
            )),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(bytes).unwrap();
        let parsed_msg = Message::from_serde_message(parsed_serde_msg).unwrap();
        assert_eq!(parsed_msg, original_msg);
    }
}
