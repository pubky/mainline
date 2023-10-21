use rand::prelude::*;
use std::convert::TryInto;
use std::net::SocketAddr;
use std::time::Duration;

use crate::{
    common::{Id, Node},
    messages, Error, Result,
};

/// Allows building [messages::Message](crate::messages::Message) structs in a more human-friendly way.
///
/// # Example
/// ```
/// use mainline::common::Id;
/// use mainline::messages::MessageBuilder;
///
/// let client_id = Id::random();
/// let server_id = Id::random();
///
/// // To build a ping request
/// let ping_req = MessageBuilder::new_ping_request()
///     .sender_id(client_id)
///     .build()
///     .unwrap();
///
/// // To build a ping response
/// let ping_res = MessageBuilder::new_ping_response()
///     .sender_id(server_id)
///     .transaction_id(ping_req.transaction_id.clone())
///     .build()
///     .unwrap();
/// ```
#[derive(Clone)]
pub struct MessageBuilder {
    message_type: BuilderMessageType,

    transaction_id: Option<Vec<u8>>,
    version: Option<Vec<u8>>,
    requester_ip: Option<SocketAddr>,
    read_only: Option<bool>,

    sender_id: Option<Id>,
    target: Option<Id>,
    port: Option<u16>,
    implied_port: Option<bool>,
    token: Option<Vec<u8>>,
    nodes: Option<Vec<Node>>,
    peers: Option<Vec<SocketAddr>>,
    interval: Option<Duration>,
    samples: Option<Vec<Id>>,
    num_infohashes: Option<usize>,
    code: Option<i32>,
    description: Option<String>,
}

/// All the different types of Message that a MesssageBuilder can build
#[derive(Clone)]
enum BuilderMessageType {
    PingRequest,
    PingResponse,
    Error,
}

impl MessageBuilder {
    /// Create a new MessageBuilder for a ping request
    pub fn new_ping_request() -> MessageBuilder {
        MessageBuilder::new(BuilderMessageType::PingRequest)
    }

    /// Create a new MessageBuilder for a ping response
    pub fn new_ping_response() -> MessageBuilder {
        MessageBuilder::new(BuilderMessageType::PingResponse)
    }

    /// Create a new MessageBuilder for an error
    pub fn new_error() -> MessageBuilder {
        MessageBuilder::new(BuilderMessageType::Error)
    }

    fn new(message_type: BuilderMessageType) -> MessageBuilder {
        MessageBuilder {
            message_type,
            transaction_id: None,
            version: None,
            requester_ip: None,
            read_only: None,
            sender_id: None,
            target: None,
            port: None,
            implied_port: None,
            token: None,
            nodes: None,
            peers: None,
            interval: None,
            samples: None,
            num_infohashes: None,
            code: None,
            description: None,
        }
    }

    /// Set the transaction id of the message. If one is not specified,
    /// generated requests will get a random transaction id and responses
    /// will receive an error.
    pub fn transaction_id(mut self, transaction_id: Vec<u8>) -> Self {
        self.transaction_id = Some(transaction_id);
        self
    }

    /// Set the string of bytes that should be included in the message to
    /// identify the version of the software participating on the DHT.
    ///
    /// If one is not specified, the builder will omit the version field
    /// from the generated message (it is optional).
    pub fn version(mut self, version: Vec<u8>) -> Self {
        self.version = Some(version);
        self
    }

    /// For response messages, set the IP address and port that we saw the
    /// request come from. This is used to help other nodes on the DHT
    /// know what their external IPv4 address is.
    ///
    /// Has no effect for request messages. If not specified on
    /// response messages, the builder will omit it from the generated
    /// response message.
    pub fn requester_ip(mut self, remote: SocketAddr) -> Self {
        self.requester_ip = Some(remote);
        self
    }

    /// For request messages, specifies whether the read only flag should be set.
    ///
    /// Has no effect on response messages.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = Some(read_only);
        self
    }

    /// Set the Id of the DHT node sending the message (whether it's a request or response).
    pub fn sender_id(mut self, sender_id: Id) -> Self {
        self.sender_id = Some(sender_id);
        self
    }

    /// Set the Id of the target node or info_hash (for get_peers, find_node,
    /// sample_infohashes, announce_peer)
    pub fn target(mut self, target: Id) -> Self {
        self.target = Some(target);
        self
    }

    /// Set the port field for announce_peer requests.
    ///
    /// If not specified, 0 will be used and implied_port will automatically
    /// be implicitly set to true (unless explicitly set to false, in which
    /// case an error will occur).
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Set the true/false value of implied port for announce_peer requests.
    pub fn implied_port(mut self, implied_port: bool) -> Self {
        self.implied_port = Some(implied_port);
        self
    }

    /// Set the token byte string. Used for announce_peer requests and
    /// get_peers responses.
    pub fn token(mut self, token: Vec<u8>) -> Self {
        self.token = Some(token);
        self
    }

    /// Set the list of Nodes used in get_peers, find_node, and
    /// sample_infohashes responses.
    ///
    /// nodes will be ignored for get_peers messages if peers are specified.
    pub fn nodes(mut self, nodes: Vec<Node>) -> Self {
        self.nodes = Some(nodes);
        self
    }

    /// Set the list of peers used in get_peers responses.
    ///
    /// nodes will be ignored for get_peers messages if peers are specified.
    pub fn peers(mut self, peers: Vec<SocketAddr>) -> Self {
        self.peers = Some(peers);
        self
    }

    /// Set the interval used in sample_infohashes responses.
    pub fn interval(mut self, interval: Duration) -> Self {
        self.interval = Some(interval);
        self
    }

    /// Set the list of info_hashes used in sample_infohashes responses.
    pub fn samples(mut self, samples: Vec<Id>) -> Self {
        self.samples = Some(samples);
        self
    }

    /// Set the number of info_hashes as reported in sample_infohashes
    /// responses.
    pub fn num_infohashes(mut self, num: usize) -> Self {
        self.num_infohashes = Some(num);
        self
    }

    /// Set the error code .for error messages.
    pub fn code(mut self, code: i32) -> Self {
        self.code = Some(code);
        self
    }

    /// Set the description for error messages.
    pub fn description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Build the Message, consuming this MessageBuilder in the process
    pub fn build(self) -> Result<messages::Message> {
        match self.message_type {
            BuilderMessageType::PingRequest => self.build_ping_request(),
            BuilderMessageType::PingResponse => self.build_ping_response(),
            BuilderMessageType::Error => self.build_error(),
        }
    }
}

macro_rules! required_or_error {
    ($self:ident, $builder_field:ident) => {
        match $self.$builder_field {
            None => {
                return Err(Error::BuilderMissingFieldError(stringify!($builder_field)));
            }
            Some($builder_field) => $builder_field,
        }
    };
}

macro_rules! build_request_common {
    ($self:ident, $x:expr) => {
        messages::Message {
            transaction_id: match $self.transaction_id {
                Some(transaction_id) => transaction_id,
                None => {
                    let mut rng = thread_rng();
                    vec![rng.gen(), rng.gen()]
                }
            },
            version: $self.version,
            requester_ip: None,
            message_type: messages::MessageType::Request($x),
            read_only: $self.read_only,
        }
    };
}

macro_rules! build_response_common {
    ($self:ident, $x:expr) => {
        messages::Message {
            transaction_id: required_or_error!($self, transaction_id),
            version: $self.version,
            requester_ip: $self.requester_ip,
            message_type: messages::MessageType::Response($x),
            read_only: None,
        }
    };
}

impl MessageBuilder {
    fn build_ping_request(self) -> Result<messages::Message> {
        Ok(build_request_common!(
            self,
            messages::RequestSpecific::PingRequest(messages::PingRequestArguments {
                requester_id: required_or_error!(self, sender_id),
            },)
        ))
    }

    fn build_ping_response(self) -> Result<messages::Message> {
        Ok(build_response_common!(
            self,
            messages::ResponseSpecific::PingResponse(messages::PingResponseArguments {
                responder_id: required_or_error!(self, sender_id),
            },)
        ))
    }

    fn build_error(self) -> Result<messages::Message> {
        Ok(messages::Message {
            transaction_id: required_or_error!(self, transaction_id),
            version: None,
            requester_ip: None,
            message_type: messages::MessageType::Error(messages::ErrorSpecific {
                code: required_or_error!(self, code),
                description: required_or_error!(self, description),
            }),
            read_only: None,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_sender_id_is_required_in_requests() {
        let transaction_id = vec![0, 1, 2, 3];
        let err = MessageBuilder::new_ping_request()
            .transaction_id(transaction_id)
            .build();
        assert!(err.is_err());
        assert!(matches!(
            err.unwrap_err(),
            Error::BuilderMissingFieldError("sender_id")
        ));
    }

    #[test]
    fn test_sender_id_is_required_in_responses() {
        let transaction_id = vec![0, 1, 2, 3];
        let err = MessageBuilder::new_ping_response()
            .transaction_id(transaction_id)
            .build();
        assert!(err.is_err());
        assert!(matches!(
            err.unwrap_err(),
            Error::BuilderMissingFieldError("sender_id")
        ));
    }

    #[test]
    fn test_transaction_id_optional_in_requests() {
        let our_id = Id::random();
        let b = MessageBuilder::new_ping_request().sender_id(our_id).build();
        assert!(b.is_ok());
        assert!(!b.unwrap().transaction_id.is_empty());
    }

    #[test]
    fn test_transaction_id_required_in_responses() {
        let our_id = Id::random();
        let err = MessageBuilder::new_ping_response()
            .sender_id(our_id)
            .build();
        assert!(err.is_err());
        assert!(matches!(
            err.unwrap_err(),
            Error::BuilderMissingFieldError("transaction_id")
        ));
    }

    #[test]
    fn test_version_field_populated() {
        let our_id = Id::random();
        let b = MessageBuilder::new_ping_request()
            .sender_id(our_id)
            .version(vec![6, 6, 6])
            .build();
        assert!(b.is_ok());
        assert_eq!(b.unwrap().version.unwrap_or_default(), vec!(6, 6, 6));
    }

    #[test]
    fn test_requester_ip_pointless_on_requests() {
        let our_id = Id::random();
        let b = MessageBuilder::new_ping_request()
            .sender_id(our_id)
            .requester_ip("1.0.1.0:53".parse().unwrap())
            .build();
        assert!(b.is_ok());
        assert_eq!(b.unwrap().requester_ip, None);
    }

    #[test]
    fn test_requester_ip_useful_on_responses() {
        let our_id = Id::random();
        let b = MessageBuilder::new_ping_response()
            .sender_id(our_id)
            .requester_ip("1.0.1.0:53".parse().unwrap())
            .transaction_id(vec![1])
            .build();
        assert!(b.is_ok());
        assert_eq!(b.unwrap().requester_ip, Some("1.0.1.0:53".parse().unwrap()));
    }

    #[test]
    fn test_read_only_useful_on_requests() {
        let our_id = Id::random();
        let b = MessageBuilder::new_ping_request()
            .sender_id(our_id)
            .read_only(true)
            .build();
        assert!(b.is_ok());
        assert_eq!(b.unwrap().read_only, Some(true));
    }

    #[test]
    fn test_read_only_pointless_on_responses() {
        let our_id = Id::random();
        let b = MessageBuilder::new_ping_response()
            .sender_id(our_id)
            .read_only(true)
            .transaction_id(vec![1])
            .build();
        assert!(b.is_ok());
        assert_eq!(b.unwrap().read_only, None);
    }

    #[test]
    fn test_build_ping_request() {
        let our_id = Id::random();
        let transaction_id = vec![0, 1, 2, 3];
        assert_eq!(
            MessageBuilder::new_ping_request()
                .sender_id(our_id)
                .transaction_id(transaction_id.clone())
                .build()
                .expect("Failed to build message"),
            messages::Message {
                transaction_id,
                version: None,
                requester_ip: None,
                message_type: messages::MessageType::Request(
                    messages::RequestSpecific::PingRequest(messages::PingRequestArguments {
                        requester_id: our_id
                    })
                ),
                read_only: None,
            }
        );
    }

    #[test]
    fn test_build_ping_response() {
        let our_id = Id::random();
        let transaction_id = vec![0, 1, 2, 3];
        assert_eq!(
            MessageBuilder::new_ping_response()
                .sender_id(our_id)
                .transaction_id(transaction_id.clone())
                .build()
                .expect("Failed to build message"),
            messages::Message {
                transaction_id,
                version: None,
                requester_ip: None,
                message_type: messages::MessageType::Response(
                    messages::ResponseSpecific::PingResponse(messages::PingResponseArguments {
                        responder_id: our_id
                    })
                ),
                read_only: None,
            }
        );
    }
}
