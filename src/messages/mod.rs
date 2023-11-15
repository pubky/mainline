//! Serealize and decerealize Krpc messages.

// Copied from <https://github.com/raptorswing/rustydht-lib/blob/main/src/packets/public.rs>

mod internal;

use std::convert::TryInto;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::common::{Id, Node, ID_SIZE};
use crate::{Error, Result};

#[derive(Debug, PartialEq, Clone)]
pub struct Message {
    pub transaction_id: u16,

    /// The version of the requester or responder.
    pub version: Option<Vec<u8>>,

    /// The IP address and port ("SocketAddr") of the requester as seen from the responder's point of view.
    /// This should be set only on response, but is defined at this level with the other common fields to avoid defining yet another layer on the response objects.
    pub requester_ip: Option<SocketAddr>,

    pub message_type: MessageType,

    /// For bep0043. When set true on a request, indicates that the requester can't reply to requests and that responders should not add requester to their routing tables.
    /// Should only be set on requests - undefined behavior when set on a response.
    pub read_only: bool,
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
    Ping(PingRequestArguments),
    FindNode(FindNodeRequestArguments),
    GetPeers(GetPeersRequestArguments),
    AnnouncePeer(AnnouncePeerRequestArguments),
}

#[derive(Debug, PartialEq, Clone)]
pub enum ResponseSpecific {
    Ping(PingResponseArguments),
    FindNode(FindNodeResponseArguments),
    GetPeers(GetPeersResponseArguments),
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

// === FIND_NODE ===
#[derive(Debug, PartialEq, Clone)]
pub struct FindNodeRequestArguments {
    pub target: Id,
    pub requester_id: Id,
}

#[derive(Debug, PartialEq, Clone)]
pub struct FindNodeResponseArguments {
    pub responder_id: Id,
    pub nodes: Vec<Node>,
}

// === Get Peers ===

#[derive(Debug, PartialEq, Clone)]
pub struct GetPeersRequestArguments {
    pub info_hash: Id,
    pub requester_id: Id,
}

#[derive(Debug, PartialEq, Clone)]
pub struct GetPeersResponseArguments {
    pub responder_id: Id,
    pub token: Vec<u8>,
    pub nodes: Option<Vec<Node>>,
    pub values: Option<Vec<SocketAddr>>,
}

// === Announce Peer ===

#[derive(Debug, PartialEq, Clone)]
pub struct AnnouncePeerRequestArguments {
    pub requester_id: Id,
    pub info_hash: Id,
    pub port: u16,
    pub implied_port: Option<bool>,
    pub token: Vec<u8>,
}

impl Message {
    fn into_serde_message(self) -> internal::DHTMessage {
        internal::DHTMessage {
            transaction_id: self.transaction_id.to_be_bytes().to_vec(),
            version: self.version,
            ip: self
                .requester_ip
                .map(|sockaddr| sockaddr_to_bytes(&sockaddr)),
            read_only: if self.read_only { Some(1) } else { Some(0) },
            variant: match self.message_type {
                MessageType::Request(req) => internal::DHTMessageVariant::Request(match req {
                    RequestSpecific::Ping(ping_args) => internal::DHTRequestSpecific::Ping {
                        arguments: internal::DHTPingRequestArguments {
                            id: ping_args.requester_id.to_vec(),
                        },
                    },
                    RequestSpecific::FindNode(find_node_args) => {
                        internal::DHTRequestSpecific::FindNode {
                            arguments: internal::DHTFindNodeRequestArguments {
                                id: find_node_args.requester_id.to_vec(),
                                target: find_node_args.target.to_vec(),
                            },
                        }
                    }
                    RequestSpecific::GetPeers(get_peers_args) => {
                        internal::DHTRequestSpecific::GetPeers {
                            arguments: internal::DHTGetPeersRequestArguments {
                                id: get_peers_args.requester_id.to_vec(),
                                info_hash: get_peers_args.info_hash.to_vec(),
                            },
                        }
                    }
                    RequestSpecific::AnnouncePeer(announce_peer_args) => {
                        internal::DHTRequestSpecific::AnnouncePeer {
                            arguments: internal::DHTAnnouncePeerRequestArguments {
                                id: announce_peer_args.requester_id.to_vec(),
                                info_hash: announce_peer_args.info_hash.to_vec(),
                                port: announce_peer_args.port,
                                implied_port: if announce_peer_args.implied_port.is_some() {
                                    Some(1)
                                } else {
                                    Some(0)
                                },
                                token: announce_peer_args.token,
                            },
                        }
                    }
                }),

                MessageType::Response(res) => internal::DHTMessageVariant::Response(match res {
                    ResponseSpecific::Ping(ping_args) => internal::DHTResponseSpecific::Ping {
                        arguments: internal::DHTPingResponseArguments {
                            id: ping_args.responder_id.to_vec(),
                        },
                    },
                    ResponseSpecific::FindNode(find_node_args) => {
                        internal::DHTResponseSpecific::FindNode {
                            arguments: internal::DHTFindNodeResponseArguments {
                                id: find_node_args.responder_id.to_vec(),
                                nodes: nodes4_to_bytes(&find_node_args.nodes),
                            },
                        }
                    }
                    ResponseSpecific::GetPeers(get_peers_args) => {
                        internal::DHTResponseSpecific::GetPeers {
                            arguments: internal::DHTGetPeersResponseArguments {
                                id: get_peers_args.responder_id.to_vec(),
                                token: get_peers_args.token.clone(),
                                nodes: get_peers_args
                                    .nodes
                                    .as_ref()
                                    .map(|nodes| nodes4_to_bytes(nodes)),
                                values: get_peers_args.values.as_ref().map(peers_to_bytes),
                            },
                        }
                    }
                }),

                MessageType::Error(err) => {
                    internal::DHTMessageVariant::Error(internal::DHTErrorSpecific {
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
            transaction_id: transaction_id(msg.transaction_id)?,
            version: msg.version,
            requester_ip: match msg.ip {
                Some(ip) => Some(bytes_to_sockaddr(ip)?),
                _ => None,
            },
            read_only: if let Some(read_only) = msg.read_only {
                read_only > 0
            } else {
                false
            },
            message_type: match msg.variant {
                internal::DHTMessageVariant::Request(req_variant) => {
                    MessageType::Request(match req_variant {
                        internal::DHTRequestSpecific::Ping { arguments } => {
                            RequestSpecific::Ping(PingRequestArguments {
                                requester_id: Id::from_bytes(arguments.id)?,
                            })
                        }
                        internal::DHTRequestSpecific::FindNode { arguments } => {
                            RequestSpecific::FindNode(FindNodeRequestArguments {
                                requester_id: Id::from_bytes(arguments.id)?,
                                target: Id::from_bytes(&arguments.target)?,
                            })
                        }
                        internal::DHTRequestSpecific::GetPeers { arguments } => {
                            RequestSpecific::GetPeers(GetPeersRequestArguments {
                                requester_id: Id::from_bytes(arguments.id)?,
                                info_hash: Id::from_bytes(&arguments.info_hash)?,
                            })
                        }
                        internal::DHTRequestSpecific::AnnouncePeer { arguments } => {
                            RequestSpecific::AnnouncePeer(AnnouncePeerRequestArguments {
                                requester_id: Id::from_bytes(arguments.id)?,
                                implied_port: if arguments.implied_port.is_none() {
                                    None
                                } else if arguments.implied_port.unwrap() != 0 {
                                    Some(true)
                                } else {
                                    Some(false)
                                },
                                info_hash: Id::from_bytes(&arguments.info_hash)?,
                                port: arguments.port,
                                token: arguments.token,
                            })
                        }
                    })
                }

                internal::DHTMessageVariant::Response(res_variant) => {
                    MessageType::Response(match res_variant {
                        internal::DHTResponseSpecific::Ping { arguments } => {
                            ResponseSpecific::Ping(PingResponseArguments {
                                responder_id: Id::from_bytes(arguments.id)?,
                            })
                        }
                        internal::DHTResponseSpecific::FindNode { arguments } => {
                            ResponseSpecific::FindNode(FindNodeResponseArguments {
                                responder_id: Id::from_bytes(&arguments.id)?,
                                nodes: bytes_to_nodes4(&arguments.nodes)?,
                            })
                        }
                        internal::DHTResponseSpecific::GetPeers { arguments } => {
                            ResponseSpecific::GetPeers(GetPeersResponseArguments {
                                responder_id: Id::from_bytes(&arguments.id)?,
                                token: arguments.token.clone(),
                                nodes: match arguments.nodes {
                                    Some(nodes) => Some(bytes_to_nodes4(nodes)?),
                                    None => None,
                                },
                                values: match arguments.values {
                                    Some(values) => Some(bytes_to_peers(values)?),
                                    None => None,
                                },
                            })
                        }
                    })
                }

                internal::DHTMessageVariant::Error(err) => {
                    if err.error_info.len() < 2 {
                        return Err(Error::Static(
                            "Error packet should have at least 2 elements",
                        ));
                    }
                    MessageType::Error(ErrorSpecific {
                        code: match err.error_info[0] {
                            serde_bencode::value::Value::Int(code) => match code.try_into() {
                                Ok(code) => code,
                                Err(_) => return Err(Error::Static("error parsing error code")),
                            },
                            _ => return Err(Error::Static("Expected error code as first element")),
                        },
                        description: match &err.error_info[1] {
                            serde_bencode::value::Value::Bytes(desc) => {
                                match std::str::from_utf8(desc) {
                                    Ok(desc) => desc.to_string(),
                                    Err(_) => {
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

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.clone().into_serde_message().to_bytes()
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
                RequestSpecific::Ping(arguments) => arguments.requester_id,
                RequestSpecific::FindNode(arguments) => arguments.requester_id,
                RequestSpecific::GetPeers(arguments) => arguments.requester_id,
                RequestSpecific::AnnouncePeer(arguments) => arguments.requester_id,
            },
            MessageType::Response(response_variant) => match response_variant {
                ResponseSpecific::Ping(arguments) => arguments.responder_id,
                ResponseSpecific::FindNode(arguments) => arguments.responder_id,
                ResponseSpecific::GetPeers(arguments) => arguments.responder_id,
            },
            MessageType::Error(_) => {
                return None;
            }
        };

        Some(id)
    }

    /// If the response contains a closer nodes to the target, return that!
    pub fn get_closer_nodes(&self) -> Option<Vec<Node>> {
        match &self.message_type {
            MessageType::Response(response_variant) => match response_variant {
                ResponseSpecific::FindNode(arguments) => Some(arguments.nodes.clone()),
                ResponseSpecific::GetPeers(arguments) => arguments.nodes.as_ref().cloned(),
                _ => None,
            },
            _ => None,
        }
    }
}

// Return the transaction Id as a u16
pub fn transaction_id(bytes: Vec<u8>) -> Result<u16> {
    if bytes.len() == 2 {
        return Ok(((bytes[0] as u16) << 8) | (bytes[1] as u16));
    } else if bytes.len() == 1 {
        return Ok(bytes[0] as u16);
    }

    Err(Error::InvalidTransactionId(bytes))
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
    bytes.extend(port_bytes);

    bytes
}

fn nodes4_to_bytes(nodes: &[Node]) -> Vec<u8> {
    let node4_byte_size: usize = ID_SIZE + 6;
    let mut vec = Vec::with_capacity(node4_byte_size * nodes.len());
    for node in nodes {
        vec.append(&mut node.id.to_vec());
        vec.append(&mut sockaddr_to_bytes(&node.address));
    }
    vec
}

fn bytes_to_nodes4<T: AsRef<[u8]>>(bytes: T) -> Result<Vec<Node>> {
    let bytes = bytes.as_ref();
    let node4_byte_size: usize = ID_SIZE + 6;
    if bytes.len() % node4_byte_size != 0 {
        return Err(Error::Generic(format!(
            "Wrong number of bytes for nodes message ({})",
            bytes.len()
        )));
    }

    let expected_num = bytes.len() / node4_byte_size;
    let mut to_ret = Vec::with_capacity(expected_num);
    for i in 0..bytes.len() / node4_byte_size {
        let i = i * node4_byte_size;
        let id = Id::from_bytes(&bytes[i..i + ID_SIZE])?;
        let sockaddr = bytes_to_sockaddr(&bytes[i + ID_SIZE..i + node4_byte_size])?;
        let node = Node::new(id, sockaddr);
        to_ret.push(node);
    }

    Ok(to_ret)
}

fn peers_to_bytes<T: AsRef<[SocketAddr]>>(peers: T) -> Vec<serde_bytes::ByteBuf> {
    let peers = peers.as_ref();
    peers
        .iter()
        .map(|p| serde_bytes::ByteBuf::from(sockaddr_to_bytes(p)))
        .collect()
}

fn bytes_to_peers<T: AsRef<[serde_bytes::ByteBuf]>>(bytes: T) -> Result<Vec<SocketAddr>> {
    let bytes = bytes.as_ref();
    bytes.iter().map(bytes_to_sockaddr).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_id() {
        assert_eq!(transaction_id(vec![255]).unwrap(), 255);
        assert_eq!(transaction_id(vec![1, 2]).unwrap(), 258);
        assert!(transaction_id(vec![]).is_err());
    }

    #[test]
    fn test_ping_request() {
        let original_msg = Message {
            transaction_id: 258,
            version: None,
            requester_ip: None,
            read_only: false,
            message_type: MessageType::Request(RequestSpecific::Ping(PingRequestArguments {
                requester_id: Id::random(),
            })),
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
            transaction_id: 258,
            version: Some(vec![0xde, 0xad]),
            requester_ip: Some("99.100.101.102:1030".parse().unwrap()),
            read_only: false,
            message_type: MessageType::Response(ResponseSpecific::Ping(PingResponseArguments {
                responder_id: Id::random(),
            })),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(bytes).unwrap();
        let parsed_msg = Message::from_serde_message(parsed_serde_msg).unwrap();
        assert_eq!(parsed_msg, original_msg);
    }

    #[test]
    fn test_find_node_request() {
        let original_msg = Message {
            transaction_id: 258,
            version: Some(vec![0x62, 0x61, 0x72, 0x66]),
            requester_ip: None,
            read_only: false,
            message_type: MessageType::Request(RequestSpecific::FindNode(
                FindNodeRequestArguments {
                    target: Id::random(),
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
    fn test_find_node_request_read_only() {
        let original_msg = Message {
            transaction_id: 258,
            version: Some(vec![0x62, 0x61, 0x72, 0x66]),
            requester_ip: None,
            read_only: true,
            message_type: MessageType::Request(RequestSpecific::FindNode(
                FindNodeRequestArguments {
                    target: Id::random(),
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
    fn test_find_node_response() {
        let original_msg = Message {
            transaction_id: 258,
            version: Some(vec![1]),
            requester_ip: Some("50.51.52.53:5455".parse().unwrap()),
            read_only: false,
            message_type: MessageType::Response(ResponseSpecific::FindNode(
                FindNodeResponseArguments {
                    responder_id: Id::random(),
                    nodes: vec![Node::new(Id::random(), "49.50.52.52:5354".parse().unwrap())],
                },
            )),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(bytes).unwrap();
        let parsed_msg = Message::from_serde_message(parsed_serde_msg).unwrap();
        assert_eq!(parsed_msg.get_author_id(), original_msg.get_author_id());
        assert_eq!(
            match parsed_msg.get_closer_nodes() {
                Some(nodes) => Some(nodes.iter().map(|n| (n.id, n.address)).collect::<Vec<_>>()),
                None => None,
            },
            match original_msg.get_closer_nodes() {
                Some(nodes) => Some(nodes.iter().map(|n| (n.id, n.address)).collect::<Vec<_>>()),
                None => None,
            },
        );
    }

    #[test]
    fn test_get_peers_request() {
        let original_msg = Message {
            transaction_id: 258,
            version: Some(vec![72, 73]),
            requester_ip: None,
            read_only: false,
            message_type: MessageType::Request(RequestSpecific::GetPeers(
                GetPeersRequestArguments {
                    info_hash: Id::random(),
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
    fn test_get_peers_response() {
        let original_msg = Message {
            transaction_id: 3,
            version: Some(vec![1]),
            requester_ip: Some("50.51.52.53:5455".parse().unwrap()),
            read_only: true,
            message_type: MessageType::Response(ResponseSpecific::GetPeers(
                GetPeersResponseArguments {
                    responder_id: Id::random(),
                    token: vec![99, 100, 101, 102],
                    nodes: Some(vec![Node::new(
                        Id::random(),
                        "49.50.52.52:5354".parse().unwrap(),
                    )]),
                    values: None,
                },
            )),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(bytes).unwrap();
        let parsed_msg = Message::from_serde_message(parsed_serde_msg).unwrap();

        assert_eq!(parsed_msg.transaction_id, original_msg.transaction_id);
        assert_eq!(parsed_msg.version, original_msg.version);
        assert_eq!(parsed_msg.requester_ip, original_msg.requester_ip);
        assert_eq!(parsed_msg.get_author_id(), original_msg.get_author_id());
        assert_eq!(
            match parsed_msg.get_closer_nodes() {
                Some(nodes) => Some(nodes.iter().map(|n| (n.id, n.address)).collect::<Vec<_>>()),
                None => None,
            },
            match original_msg.get_closer_nodes() {
                Some(nodes) => Some(nodes.iter().map(|n| (n.id, n.address)).collect::<Vec<_>>()),
                None => None,
            },
        );
    }

    #[test]
    fn test_get_peers_response_peers() {
        let original_msg = Message {
            transaction_id: 3,
            version: Some(vec![1]),
            requester_ip: Some("50.51.52.53:5455".parse().unwrap()),
            read_only: false,
            message_type: MessageType::Response(ResponseSpecific::GetPeers(
                GetPeersResponseArguments {
                    responder_id: Id::random(),
                    token: vec![99, 100, 101, 102],
                    nodes: None,
                    values: Some(vec!["123.123.123.123:123".parse().unwrap()]),
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
    fn test_get_peers_response_neither() {
        let serde_message = internal::DHTMessage {
            ip: None,
            read_only: None,
            transaction_id: vec![1, 2],
            version: None,
            variant: internal::DHTMessageVariant::Response(
                internal::DHTResponseSpecific::GetPeers {
                    arguments: internal::DHTGetPeersResponseArguments {
                        id: Id::random().to_vec(),
                        token: vec![0, 1],
                        nodes: None,
                        values: None,
                    },
                },
            ),
        };
        let parsed_msg = Message::from_serde_message(serde_message).unwrap();
        assert!(matches!(
            parsed_msg.message_type,
            MessageType::Response(ResponseSpecific::GetPeers(GetPeersResponseArguments { .. }))
        ));
    }
}
