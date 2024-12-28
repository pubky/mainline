//! Serealize and decerealize Krpc messages.

// Copied from <https://github.com/raptorswing/rustydht-lib/blob/main/src/packets/public.rs>

mod internal;

use std::convert::TryInto;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::rc::Rc;

use crate::common::{Id, Node, ID_SIZE};

use super::InvalidIdSize;

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
pub struct RequestSpecific {
    pub requester_id: Id,
    pub request_type: RequestTypeSpecific,
}

#[derive(Debug, PartialEq, Clone)]
pub enum RequestTypeSpecific {
    Ping,
    FindNode(FindNodeRequestArguments),
    GetPeers(GetPeersRequestArguments),
    GetValue(GetValueRequestArguments),

    Put(PutRequest),
}

#[derive(Debug, PartialEq, Clone)]
pub struct PutRequest {
    pub token: Box<[u8]>,
    pub put_request_type: PutRequestSpecific,
}

#[derive(Debug, PartialEq, Clone)]
pub enum PutRequestSpecific {
    AnnouncePeer(AnnouncePeerRequestArguments),
    PutImmutable(PutImmutableRequestArguments),
    PutMutable(PutMutableRequestArguments),
}

#[derive(Debug, PartialEq, Clone)]
pub enum ResponseSpecific {
    Ping(PingResponseArguments),
    FindNode(FindNodeResponseArguments),
    GetPeers(GetPeersResponseArguments),
    GetImmutable(GetImmutableResponseArguments),
    GetMutable(GetMutableResponseArguments),
    NoValues(NoValuesResponseArguments),
    NoMoreRecentValue(NoMoreRecentValueResponseArguments),
}

// === PING ===
#[derive(Debug, PartialEq, Clone)]
pub struct PingResponseArguments {
    pub responder_id: Id,
}

// === FIND_NODE ===
#[derive(Debug, PartialEq, Clone)]
pub struct FindNodeRequestArguments {
    pub target: Id,
}

#[derive(Debug, PartialEq, Clone)]
pub struct FindNodeResponseArguments {
    pub responder_id: Id,
    pub nodes: Vec<Rc<Node>>,
}

// Get anything

#[derive(Debug, PartialEq, Clone)]
pub struct GetValueRequestArguments {
    pub target: Id,
    pub seq: Option<i64>,
    // A bit of a hack, using this to carry an optional
    // salt in the query.request field of [crate::query]
    // not really encoded, decoded or sent over the wire.
    pub salt: Option<Box<[u8]>>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct NoValuesResponseArguments {
    pub responder_id: Id,
    pub token: Box<[u8]>,
    pub nodes: Option<Vec<Rc<Node>>>,
}

// === Get Peers ===

#[derive(Debug, PartialEq, Clone)]
pub struct GetPeersRequestArguments {
    pub info_hash: Id,
}

#[derive(Debug, PartialEq, Clone)]
pub struct GetPeersResponseArguments {
    pub responder_id: Id,
    pub token: Box<[u8]>,
    pub values: Vec<SocketAddr>,
    pub nodes: Option<Vec<Rc<Node>>>,
}

// === Announce Peer ===

#[derive(Debug, PartialEq, Clone)]
pub struct AnnouncePeerRequestArguments {
    pub info_hash: Id,
    pub port: u16,
    pub implied_port: Option<bool>,
}

// === Get Immutable ===

#[derive(Debug, PartialEq, Clone)]
pub struct GetImmutableResponseArguments {
    pub responder_id: Id,
    pub token: Box<[u8]>,
    pub nodes: Option<Vec<Rc<Node>>>,
    pub v: Vec<u8>,
}

// === Get Mutable ===

#[derive(Debug, PartialEq, Clone)]
pub struct GetMutableResponseArguments {
    pub responder_id: Id,
    pub token: Box<[u8]>,
    pub nodes: Option<Vec<Rc<Node>>>,
    pub v: Vec<u8>,
    pub k: [u8; 32],
    pub seq: i64,
    pub sig: [u8; 64],
}

#[derive(Debug, PartialEq, Clone)]
pub struct NoMoreRecentValueResponseArguments {
    pub responder_id: Id,
    pub token: Box<[u8]>,
    pub nodes: Option<Vec<Rc<Node>>>,
    pub seq: i64,
}

// === Put Immutable ===

#[derive(Debug, PartialEq, Clone)]
pub struct PutImmutableRequestArguments {
    pub target: Id,
    pub v: Vec<u8>,
}

// === Put Mutable ===

#[derive(Debug, PartialEq, Clone)]
pub struct PutMutableRequestArguments {
    pub target: Id,
    pub v: Vec<u8>,
    pub k: [u8; 32],
    pub seq: i64,
    pub sig: [u8; 64],
    pub salt: Option<Vec<u8>>,
    pub cas: Option<i64>,
}

impl Message {
    fn into_serde_message(self) -> internal::DHTMessage {
        internal::DHTMessage {
            transaction_id: self.transaction_id.to_be_bytes(),
            version: self.version,
            ip: self
                .requester_ip
                .map(|sockaddr| sockaddr_to_bytes(&sockaddr)),
            read_only: if self.read_only { Some(1) } else { Some(0) },
            variant: match self.message_type {
                MessageType::Request(RequestSpecific {
                    requester_id,
                    request_type,
                }) => internal::DHTMessageVariant::Request(match request_type {
                    RequestTypeSpecific::Ping => internal::DHTRequestSpecific::Ping {
                        arguments: internal::DHTPingRequestArguments {
                            id: requester_id.into(),
                        },
                    },
                    RequestTypeSpecific::FindNode(find_node_args) => {
                        internal::DHTRequestSpecific::FindNode {
                            arguments: internal::DHTFindNodeRequestArguments {
                                id: requester_id.into(),
                                target: find_node_args.target.into(),
                            },
                        }
                    }
                    RequestTypeSpecific::GetPeers(get_peers_args) => {
                        internal::DHTRequestSpecific::GetPeers {
                            arguments: internal::DHTGetPeersRequestArguments {
                                id: requester_id.into(),
                                info_hash: get_peers_args.info_hash.to_vec(),
                            },
                        }
                    }
                    RequestTypeSpecific::GetValue(get_mutable_args) => {
                        internal::DHTRequestSpecific::GetValue {
                            arguments: internal::DHTGetValueRequestArguments {
                                id: requester_id.into(),
                                target: get_mutable_args.target.into(),
                                seq: get_mutable_args.seq,
                            },
                        }
                    }
                    RequestTypeSpecific::Put(PutRequest {
                        token,
                        put_request_type,
                    }) => match put_request_type {
                        PutRequestSpecific::AnnouncePeer(announce_peer_args) => {
                            internal::DHTRequestSpecific::AnnouncePeer {
                                arguments: internal::DHTAnnouncePeerRequestArguments {
                                    id: requester_id.into(),
                                    token,

                                    info_hash: announce_peer_args.info_hash.to_vec(),
                                    port: announce_peer_args.port,
                                    implied_port: if announce_peer_args.implied_port.is_some() {
                                        Some(1)
                                    } else {
                                        Some(0)
                                    },
                                },
                            }
                        }
                        PutRequestSpecific::PutImmutable(put_immutable_arguments) => {
                            internal::DHTRequestSpecific::PutValue {
                                arguments: internal::DHTPutValueRequestArguments {
                                    id: requester_id.into(),
                                    token,

                                    target: put_immutable_arguments.target.into(),
                                    v: put_immutable_arguments.v,
                                    k: None,
                                    seq: None,
                                    sig: None,
                                    salt: None,
                                    cas: None,
                                },
                            }
                        }
                        PutRequestSpecific::PutMutable(put_mutable_arguments) => {
                            internal::DHTRequestSpecific::PutValue {
                                arguments: internal::DHTPutValueRequestArguments {
                                    id: requester_id.into(),
                                    token,

                                    target: put_mutable_arguments.target.into(),
                                    v: put_mutable_arguments.v,
                                    k: Some(put_mutable_arguments.k),
                                    seq: Some(put_mutable_arguments.seq),
                                    sig: Some(put_mutable_arguments.sig),
                                    salt: put_mutable_arguments.salt,
                                    cas: put_mutable_arguments.cas,
                                },
                            }
                        }
                    },
                }),

                MessageType::Response(res) => internal::DHTMessageVariant::Response(match res {
                    ResponseSpecific::Ping(ping_args) => internal::DHTResponseSpecific::Ping {
                        arguments: internal::DHTPingResponseArguments {
                            id: ping_args.responder_id.into(),
                        },
                    },
                    ResponseSpecific::FindNode(find_node_args) => {
                        internal::DHTResponseSpecific::FindNode {
                            arguments: internal::DHTFindNodeResponseArguments {
                                id: find_node_args.responder_id.into(),
                                nodes: nodes4_to_bytes(&find_node_args.nodes),
                            },
                        }
                    }
                    ResponseSpecific::GetPeers(get_peers_args) => {
                        internal::DHTResponseSpecific::GetPeers {
                            arguments: internal::DHTGetPeersResponseArguments {
                                id: get_peers_args.responder_id.into(),
                                token: get_peers_args.token,
                                nodes: get_peers_args
                                    .nodes
                                    .as_ref()
                                    .map(|nodes| nodes4_to_bytes(nodes)),
                                values: peers_to_bytes(get_peers_args.values),
                            },
                        }
                    }
                    ResponseSpecific::NoValues(no_values_arguments) => {
                        internal::DHTResponseSpecific::NoValues {
                            arguments: internal::DHTNoValuesResponseArguments {
                                id: no_values_arguments.responder_id.into(),
                                token: no_values_arguments.token,
                                nodes: no_values_arguments
                                    .nodes
                                    .as_ref()
                                    .map(|nodes| nodes4_to_bytes(nodes)),
                            },
                        }
                    }
                    ResponseSpecific::GetImmutable(get_immutable_args) => {
                        internal::DHTResponseSpecific::GetImmutable {
                            arguments: internal::DHTGetImmutableResponseArguments {
                                id: get_immutable_args.responder_id.into(),
                                token: get_immutable_args.token,
                                nodes: get_immutable_args
                                    .nodes
                                    .as_ref()
                                    .map(|nodes| nodes4_to_bytes(nodes)),
                                v: get_immutable_args.v,
                            },
                        }
                    }
                    ResponseSpecific::GetMutable(get_mutable_args) => {
                        internal::DHTResponseSpecific::GetMutable {
                            arguments: internal::DHTGetMutableResponseArguments {
                                id: get_mutable_args.responder_id.into(),
                                token: get_mutable_args.token,
                                nodes: get_mutable_args
                                    .nodes
                                    .as_ref()
                                    .map(|nodes| nodes4_to_bytes(nodes)),
                                v: get_mutable_args.v,
                                k: get_mutable_args.k,
                                seq: get_mutable_args.seq,
                                sig: get_mutable_args.sig,
                            },
                        }
                    }
                    ResponseSpecific::NoMoreRecentValue(args) => {
                        internal::DHTResponseSpecific::NoMoreRecentValue {
                            arguments: internal::DHTNoMoreRecentValueResponseArguments {
                                id: args.responder_id.into(),
                                token: args.token,
                                nodes: args.nodes.as_ref().map(|nodes| nodes4_to_bytes(nodes)),
                                seq: args.seq,
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

    fn from_serde_message(msg: internal::DHTMessage) -> Result<Message, DecodeMessageError> {
        Ok(Message {
            transaction_id: u16::from_be_bytes(msg.transaction_id),
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
                        internal::DHTRequestSpecific::Ping { arguments } => RequestSpecific {
                            requester_id: Id::from_bytes(arguments.id)?,
                            request_type: RequestTypeSpecific::Ping,
                        },
                        internal::DHTRequestSpecific::FindNode { arguments } => RequestSpecific {
                            requester_id: Id::from_bytes(arguments.id)?,
                            request_type: RequestTypeSpecific::FindNode(FindNodeRequestArguments {
                                target: Id::from_bytes(arguments.target)?,
                            }),
                        },
                        internal::DHTRequestSpecific::GetPeers { arguments } => RequestSpecific {
                            requester_id: Id::from_bytes(arguments.id)?,
                            request_type: RequestTypeSpecific::GetPeers(GetPeersRequestArguments {
                                info_hash: Id::from_bytes(arguments.info_hash)?,
                            }),
                        },
                        internal::DHTRequestSpecific::GetValue { arguments } => RequestSpecific {
                            requester_id: Id::from_bytes(arguments.id)?,

                            request_type: RequestTypeSpecific::GetValue(GetValueRequestArguments {
                                target: Id::from_bytes(arguments.target)?,
                                seq: arguments.seq,
                                salt: None,
                            }),
                        },
                        internal::DHTRequestSpecific::AnnouncePeer { arguments } => {
                            RequestSpecific {
                                requester_id: Id::from_bytes(arguments.id)?,
                                request_type: RequestTypeSpecific::Put(PutRequest {
                                    token: arguments.token,
                                    put_request_type: PutRequestSpecific::AnnouncePeer(
                                        AnnouncePeerRequestArguments {
                                            implied_port: arguments
                                                .implied_port
                                                .map(|implied_port| implied_port != 0),
                                            info_hash: Id::from_bytes(&arguments.info_hash)?,
                                            port: arguments.port,
                                        },
                                    ),
                                }),
                            }
                        }
                        internal::DHTRequestSpecific::PutValue { arguments } => {
                            if let Some(k) = arguments.k {
                                RequestSpecific {
                                    requester_id: Id::from_bytes(arguments.id)?,

                                    request_type: RequestTypeSpecific::Put(PutRequest {
                                        token: arguments.token,
                                        put_request_type: PutRequestSpecific::PutMutable(
                                            PutMutableRequestArguments {
                                                target: Id::from_bytes(arguments.target)?,
                                                v: arguments.v,
                                                k,
                                                // Should panic if missing.
                                                seq: arguments.seq.expect(
                                                    "Put mutable message to have sequence number",
                                                ),
                                                sig: arguments.sig.expect(
                                                    "Put mutable message to have a signature",
                                                ),
                                                salt: arguments.salt,
                                                cas: arguments.cas,
                                            },
                                        ),
                                    }),
                                }
                            } else {
                                RequestSpecific {
                                    requester_id: Id::from_bytes(arguments.id)?,

                                    request_type: RequestTypeSpecific::Put(PutRequest {
                                        token: arguments.token,
                                        put_request_type: PutRequestSpecific::PutImmutable(
                                            PutImmutableRequestArguments {
                                                target: Id::from_bytes(arguments.target)?,
                                                v: arguments.v,
                                            },
                                        ),
                                    }),
                                }
                            }
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
                                responder_id: Id::from_bytes(arguments.id)?,
                                nodes: bytes_to_nodes4(&arguments.nodes)?,
                            })
                        }
                        internal::DHTResponseSpecific::GetPeers { arguments } => {
                            ResponseSpecific::GetPeers(GetPeersResponseArguments {
                                responder_id: Id::from_bytes(arguments.id)?,
                                token: arguments.token,
                                nodes: match arguments.nodes {
                                    Some(nodes) => Some(bytes_to_nodes4(nodes)?),
                                    None => None,
                                },
                                values: bytes_to_peers(arguments.values)?,
                            })
                        }
                        internal::DHTResponseSpecific::NoValues { arguments } => {
                            ResponseSpecific::NoValues(NoValuesResponseArguments {
                                responder_id: Id::from_bytes(arguments.id)?,
                                token: arguments.token,
                                nodes: match arguments.nodes {
                                    Some(nodes) => Some(bytes_to_nodes4(nodes)?),
                                    None => None,
                                },
                            })
                        }
                        internal::DHTResponseSpecific::GetImmutable { arguments } => {
                            ResponseSpecific::GetImmutable(GetImmutableResponseArguments {
                                responder_id: Id::from_bytes(arguments.id)?,
                                token: arguments.token,
                                nodes: match arguments.nodes {
                                    Some(nodes) => Some(bytes_to_nodes4(nodes)?),
                                    None => None,
                                },
                                v: arguments.v,
                            })
                        }
                        internal::DHTResponseSpecific::GetMutable { arguments } => {
                            ResponseSpecific::GetMutable(GetMutableResponseArguments {
                                responder_id: Id::from_bytes(arguments.id)?,
                                token: arguments.token,
                                nodes: match arguments.nodes {
                                    Some(nodes) => Some(bytes_to_nodes4(nodes)?),
                                    None => None,
                                },
                                v: arguments.v,
                                k: arguments.k,
                                seq: arguments.seq,
                                sig: arguments.sig,
                            })
                        }
                        internal::DHTResponseSpecific::NoMoreRecentValue { arguments } => {
                            ResponseSpecific::NoMoreRecentValue(
                                NoMoreRecentValueResponseArguments {
                                    responder_id: Id::from_bytes(arguments.id)?,
                                    token: arguments.token,
                                    nodes: match arguments.nodes {
                                        Some(nodes) => Some(bytes_to_nodes4(nodes)?),
                                        None => None,
                                    },
                                    seq: arguments.seq,
                                },
                            )
                        }
                    })
                }

                internal::DHTMessageVariant::Error(err) => {
                    if err.error_info.len() < 2 {
                        return Err(DecodeMessageError::InvalidErrorDescription);
                    }
                    MessageType::Error(ErrorSpecific {
                        code: match err.error_info[0] {
                            serde_bencode::value::Value::Int(code) => match code.try_into() {
                                Ok(code) => code,
                                Err(_) => return Err(DecodeMessageError::InvalidErrorCode),
                            },
                            _ => return Err(DecodeMessageError::InvalidErrorCode),
                        },
                        description: match &err.error_info[1] {
                            serde_bencode::value::Value::Bytes(desc) => {
                                match std::str::from_utf8(desc) {
                                    Ok(desc) => desc.to_string(),
                                    Err(_) => {
                                        return Err(DecodeMessageError::InvalidErrorDescription)
                                    }
                                }
                            }
                            _ => return Err(DecodeMessageError::InvalidErrorDescription),
                        },
                    })
                }
            },
        })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_bencode::Error> {
        self.clone().into_serde_message().to_bytes()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Message, DecodeMessageError> {
        if bytes.len() < 15 {
            return Err(DecodeMessageError::TooShort);
        } else if bytes[0] != 100 {
            return Err(DecodeMessageError::NotBencodeDictionary);
        }

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
            MessageType::Request(arguments) => arguments.requester_id,
            MessageType::Response(response_variant) => match response_variant {
                ResponseSpecific::Ping(arguments) => arguments.responder_id,
                ResponseSpecific::FindNode(arguments) => arguments.responder_id,
                ResponseSpecific::GetPeers(arguments) => arguments.responder_id,
                ResponseSpecific::GetImmutable(arguments) => arguments.responder_id,
                ResponseSpecific::GetMutable(arguments) => arguments.responder_id,
                ResponseSpecific::NoValues(arguments) => arguments.responder_id,
                ResponseSpecific::NoMoreRecentValue(arguments) => arguments.responder_id,
            },
            MessageType::Error(_) => {
                return None;
            }
        };

        Some(id)
    }

    /// If the response contains a closer nodes to the target, return that!
    pub fn get_closer_nodes(&self) -> Option<&Vec<Rc<Node>>> {
        match &self.message_type {
            MessageType::Response(response_variant) => match response_variant {
                ResponseSpecific::Ping(_) => None,
                ResponseSpecific::FindNode(arguments) => Some(&arguments.nodes),
                ResponseSpecific::GetPeers(arguments) => arguments.nodes.as_ref(),
                ResponseSpecific::GetMutable(arguments) => arguments.nodes.as_ref(),
                ResponseSpecific::GetImmutable(arguments) => arguments.nodes.as_ref(),
                ResponseSpecific::NoValues(arguments) => arguments.nodes.as_ref(),
                ResponseSpecific::NoMoreRecentValue(arguments) => arguments.nodes.as_ref(),
            },
            _ => None,
        }
    }

    pub fn get_token(&self) -> Option<(Id, &[u8])> {
        match &self.message_type {
            MessageType::Response(response_variant) => match response_variant {
                ResponseSpecific::Ping(_) => None,
                ResponseSpecific::FindNode(_) => None,
                ResponseSpecific::GetPeers(arguments) => {
                    Some((arguments.responder_id, &arguments.token))
                }
                ResponseSpecific::GetImmutable(arguments) => {
                    Some((arguments.responder_id, &arguments.token))
                }
                ResponseSpecific::GetMutable(arguments) => {
                    Some((arguments.responder_id, &arguments.token))
                }
                ResponseSpecific::NoValues(arguments) => {
                    Some((arguments.responder_id, &arguments.token))
                }
                ResponseSpecific::NoMoreRecentValue(arguments) => {
                    Some((arguments.responder_id, &arguments.token))
                }
            },
            _ => None,
        }
    }
}

fn bytes_to_sockaddr<T: AsRef<[u8]>>(bytes: T) -> Result<SocketAddr, DecodeMessageError> {
    let bytes = bytes.as_ref();
    match bytes.len() {
        6 => {
            let ip = Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]);

            let port_bytes_as_array: [u8; 2] = bytes[4..6]
                .try_into()
                .map_err(|_| DecodeMessageError::InvalidPortEncoding)?;

            let port: u16 = u16::from_be_bytes(port_bytes_as_array);

            Ok(SocketAddr::new(IpAddr::V4(ip), port))
        }

        18 => Err(DecodeMessageError::Ipv6Unsupported),

        _ => Err(DecodeMessageError::InvalidSocketAddrEncodingLength),
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

fn nodes4_to_bytes(nodes: &[Rc<Node>]) -> Vec<u8> {
    let node4_byte_size: usize = ID_SIZE + 6;
    let mut vec = Vec::with_capacity(node4_byte_size * nodes.len());
    for node in nodes {
        vec.append(&mut node.id.to_vec());
        vec.append(&mut sockaddr_to_bytes(&node.address));
    }
    vec
}

fn bytes_to_nodes4<T: AsRef<[u8]>>(bytes: T) -> Result<Vec<Rc<Node>>, DecodeMessageError> {
    let bytes = bytes.as_ref();
    let node4_byte_size: usize = ID_SIZE + 6;
    if bytes.len() % node4_byte_size != 0 {
        return Err(DecodeMessageError::InvalidNodes4);
    }

    let expected_num = bytes.len() / node4_byte_size;
    let mut to_ret = Vec::with_capacity(expected_num);
    for i in 0..bytes.len() / node4_byte_size {
        let i = i * node4_byte_size;
        let id = Id::from_bytes(&bytes[i..i + ID_SIZE])?;
        let sockaddr = bytes_to_sockaddr(&bytes[i + ID_SIZE..i + node4_byte_size])?;
        let node = Node::new(id, sockaddr);
        to_ret.push(node.into());
    }

    Ok(to_ret)
}

fn peers_to_bytes(peers: Vec<SocketAddr>) -> Vec<serde_bytes::ByteBuf> {
    peers
        .iter()
        .map(|p| serde_bytes::ByteBuf::from(sockaddr_to_bytes(p)))
        .collect()
}

fn bytes_to_peers<T: AsRef<[serde_bytes::ByteBuf]>>(
    bytes: T,
) -> Result<Vec<SocketAddr>, DecodeMessageError> {
    let bytes = bytes.as_ref();
    bytes.iter().map(bytes_to_sockaddr).collect()
}

#[derive(thiserror::Error, Debug)]
/// Mainline crate error enum.
pub enum DecodeMessageError {
    #[error("Expected message to be longer than 15 characters")]
    TooShort,

    #[error("Expected message to start with 'd'")]
    NotBencodeDictionary,

    #[error("Wrong number of bytes for nodes")]
    InvalidNodes4,

    #[error("wrong number of bytes for port")]
    InvalidPortEncoding,

    #[error("IPv6 is not yet implemented")]
    Ipv6Unsupported,

    #[error("Wrong number of bytes for sockaddr")]
    InvalidSocketAddrEncodingLength,

    #[error("Failed to parse packet bytes: {0}")]
    BencodeError(#[from] serde_bencode::Error),

    #[error(transparent)]
    InvalidIdSize(#[from] InvalidIdSize),

    #[error("Error packet should have at least 2 elements")]
    InvalidErrorPacket,

    #[error("error parsing error code")]
    InvalidErrorCode,

    #[error("error parsing error description")]
    InvalidErrorDescription,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ping_request() {
        let original_msg = Message {
            transaction_id: 258,
            version: None,
            requester_ip: None,
            read_only: false,
            message_type: MessageType::Request(RequestSpecific {
                requester_id: Id::random(),
                request_type: RequestTypeSpecific::Ping,
            }),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(&bytes).unwrap();
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
        let parsed_serde_msg = internal::DHTMessage::from_bytes(&bytes).unwrap();
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
            message_type: MessageType::Request(RequestSpecific {
                requester_id: Id::random(),
                request_type: RequestTypeSpecific::FindNode(FindNodeRequestArguments {
                    target: Id::random(),
                }),
            }),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(&bytes).unwrap();
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
            message_type: MessageType::Request(RequestSpecific {
                requester_id: Id::random(),
                request_type: RequestTypeSpecific::FindNode(FindNodeRequestArguments {
                    target: Id::random(),
                }),
            }),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(&bytes).unwrap();
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
                    nodes: vec![Node::new(Id::random(), "49.50.52.52:5354".parse().unwrap()).into()],
                },
            )),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(&bytes).unwrap();
        let parsed_msg = Message::from_serde_message(parsed_serde_msg).unwrap();
        assert_eq!(parsed_msg.get_author_id(), original_msg.get_author_id());
        assert_eq!(
            parsed_msg
                .get_closer_nodes()
                .map(|nodes| nodes.iter().map(|n| (n.id, n.address)).collect::<Vec<_>>()),
            original_msg
                .get_closer_nodes()
                .map(|nodes| nodes.iter().map(|n| (n.id, n.address)).collect::<Vec<_>>())
        );
    }

    #[test]
    fn test_get_peers_request() {
        let original_msg = Message {
            transaction_id: 258,
            version: Some(vec![72, 73]),
            requester_ip: None,
            read_only: false,
            message_type: MessageType::Request(RequestSpecific {
                requester_id: Id::random(),
                request_type: RequestTypeSpecific::GetPeers(GetPeersRequestArguments {
                    info_hash: Id::random(),
                }),
            }),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(&bytes).unwrap();
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
            message_type: MessageType::Response(ResponseSpecific::NoValues(
                NoValuesResponseArguments {
                    responder_id: Id::random(),
                    token: vec![99, 100, 101, 102].into_boxed_slice(),
                    nodes: Some(vec![Node::new(
                        Id::random(),
                        "49.50.52.52:5354".parse().unwrap(),
                    )
                    .into()]),
                },
            )),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(&bytes).unwrap();
        let parsed_msg = Message::from_serde_message(parsed_serde_msg).unwrap();

        assert_eq!(parsed_msg.transaction_id, original_msg.transaction_id);
        assert_eq!(parsed_msg.version, original_msg.version);
        assert_eq!(parsed_msg.requester_ip, original_msg.requester_ip);
        assert_eq!(parsed_msg.get_author_id(), original_msg.get_author_id());
        assert_eq!(
            parsed_msg
                .get_closer_nodes()
                .map(|nodes| nodes.iter().map(|n| (n.id, n.address)).collect::<Vec<_>>()),
            original_msg
                .get_closer_nodes()
                .map(|nodes| nodes.iter().map(|n| (n.id, n.address)).collect::<Vec<_>>())
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
                    token: vec![99, 100, 101, 102].into_boxed_slice(),
                    nodes: None,
                    values: vec!["123.123.123.123:123".parse().unwrap()],
                },
            )),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(&bytes).unwrap();
        let parsed_msg = Message::from_serde_message(parsed_serde_msg).unwrap();
        assert_eq!(parsed_msg, original_msg);
    }

    #[test]
    fn test_get_peers_response_neither() {
        let serde_message = internal::DHTMessage {
            ip: None,
            read_only: None,
            transaction_id: [1, 2],
            version: None,
            variant: internal::DHTMessageVariant::Response(
                internal::DHTResponseSpecific::NoValues {
                    arguments: internal::DHTNoValuesResponseArguments {
                        id: Id::random().into(),
                        token: vec![0, 1].into_boxed_slice(),
                        nodes: None,
                    },
                },
            ),
        };
        let parsed_msg = Message::from_serde_message(serde_message).unwrap();
        assert!(matches!(
            parsed_msg.message_type,
            MessageType::Response(ResponseSpecific::NoValues(NoValuesResponseArguments { .. }))
        ));
    }

    #[test]
    fn test_get_immutable_request() {
        let original_msg = Message {
            transaction_id: 258,
            version: Some(vec![72, 73]),
            requester_ip: None,
            read_only: false,
            message_type: MessageType::Request(RequestSpecific {
                requester_id: Id::random(),
                request_type: RequestTypeSpecific::GetValue(GetValueRequestArguments {
                    target: Id::random(),
                    seq: Some(1231),
                    salt: None,
                }),
            }),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(&bytes).unwrap();
        let parsed_msg = Message::from_serde_message(parsed_serde_msg).unwrap();
        assert_eq!(parsed_msg, original_msg);
    }

    #[test]
    fn test_get_immutable_response() {
        let original_msg = Message {
            transaction_id: 3,
            version: Some(vec![1]),
            requester_ip: Some("50.51.52.53:5455".parse().unwrap()),
            read_only: false,
            message_type: MessageType::Response(ResponseSpecific::GetImmutable(
                GetImmutableResponseArguments {
                    responder_id: Id::random(),
                    token: vec![99, 100, 101, 102].into_boxed_slice(),
                    nodes: None,
                    v: vec![99, 100, 101, 102],
                },
            )),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(&bytes).unwrap();
        let parsed_msg = Message::from_serde_message(parsed_serde_msg).unwrap();
        assert_eq!(parsed_msg, original_msg);
    }

    #[test]
    fn test_put_immutable_request() {
        let original_msg = Message {
            transaction_id: 3,
            version: Some(vec![1]),
            requester_ip: Some("50.51.52.53:5455".parse().unwrap()),
            read_only: false,
            message_type: MessageType::Request(RequestSpecific {
                requester_id: Id::random(),
                request_type: RequestTypeSpecific::Put(PutRequest {
                    token: vec![99, 100, 101, 102].into_boxed_slice(),
                    put_request_type: PutRequestSpecific::PutImmutable(
                        PutImmutableRequestArguments {
                            target: Id::random(),
                            v: vec![99, 100, 101, 102],
                        },
                    ),
                }),
            }),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(&bytes).unwrap();
        let parsed_msg = Message::from_serde_message(parsed_serde_msg).unwrap();
        assert_eq!(parsed_msg, original_msg);
    }

    #[test]
    fn test_put_mutable_request() {
        let original_msg = Message {
            transaction_id: 3,
            version: Some(vec![1]),
            requester_ip: Some("50.51.52.53:5455".parse().unwrap()),
            read_only: false,
            message_type: MessageType::Request(RequestSpecific {
                requester_id: Id::random(),
                request_type: RequestTypeSpecific::Put(PutRequest {
                    token: vec![99, 100, 101, 102].into_boxed_slice(),
                    put_request_type: PutRequestSpecific::PutMutable(PutMutableRequestArguments {
                        target: Id::random(),
                        v: vec![99, 100, 101, 102],
                        k: [100; 32],
                        seq: 100,
                        sig: [0; 64],
                        salt: Some(vec![0, 2, 4, 8]),
                        cas: Some(100),
                    }),
                }),
            }),
        };

        let serde_msg = original_msg.clone().into_serde_message();
        let bytes = serde_msg.to_bytes().unwrap();
        let parsed_serde_msg = internal::DHTMessage::from_bytes(&bytes).unwrap();
        let parsed_msg = Message::from_serde_message(parsed_serde_msg).unwrap();
        assert_eq!(parsed_msg, original_msg);
    }
}
