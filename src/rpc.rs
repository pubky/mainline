use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::common::Id;
use crate::messages::{
    FindNodeRequestArguments, Message, MessageType, PingRequestArguments, PingResponseArguments,
    RequestSpecific, ResponseSpecific,
};
use crate::{Error, Result};

const DEFAULT_PORT: u16 = 6881;
const DEFAULT_TIMEOUT: u16 = 6881;

#[derive(Debug, Clone)]
struct Rpc {
    id: Id,
    socket: Arc<UdpSocket>,
    next_tid: u16,
    outstanding_requests: Arc<Mutex<HashMap<u16, Sender<Message>>>>,
}

impl Rpc {
    fn new() -> Result<Rpc> {
        // TODO: One day I might implement BEP42.
        let id = Id::random();

        let socket = match UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT))) {
            Ok(socket) => Ok(socket),
            Err(_) => UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))),
        }?;

        socket.set_read_timeout(Some(Duration::from_millis(100)))?;

        Ok(Rpc {
            id,
            socket: socket.into(),
            next_tid: 0,
            outstanding_requests: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Returns the address the server is listening to.
    #[inline]
    pub fn server_addr(&self) -> SocketAddr {
        self.socket.local_addr().unwrap()
    }

    /// Increments self.next_tid and returns the previous value.
    /// Wraps around at u16::max_value();
    fn tid(&mut self) -> u16 {
        let tid = self.next_tid;
        self.next_tid = self.next_tid.wrapping_add(1);
        tid
    }

    /// === Responses methods ===

    fn respond(
        &self,
        transaction_id: u16,
        to: SocketAddr,
        response: ResponseSpecific,
    ) -> Result<()> {
        let socket = self.socket.try_clone()?;

        let response = Message {
            transaction_id,
            message_type: MessageType::Response(response),
            // TODO: define our version.
            version: None,
            // TODO: configure. if false, we should disable respond or make it noop.
            read_only: Some(false),
            // Only relevant in responses.
            requester_ip: Some(to),
        };

        let bytes = &response.clone().to_bytes()?;
        socket.send_to(bytes, to)?;

        Ok(())
    }

    fn recv_from(&self) -> Result<(Message, SocketAddr)> {
        let mut buf = [0u8; 1024];
        let (amt, from) = self.socket.recv_from(&mut buf)?;
        let mut message = Message::from_bytes(&buf[..amt])?;

        match message.message_type {
            MessageType::Request(_) => {}
            // Response or error, send it to the outstanding_request
            _ => {
                if let Some(response_tx) = self
                    .outstanding_requests
                    .clone()
                    .lock()
                    .unwrap()
                    .remove(&message.transaction_id)
                {
                    response_tx.send(message.clone());
                }
            }
        };

        return Ok((message, from));
    }

    /// === Requests methods ===

    /// Create a new request message with the given request specific arguments.
    fn request_message(&mut self, transaction_id: u16, request: RequestSpecific) -> Message {
        Message {
            transaction_id,
            message_type: MessageType::Request(request),
            // TODO: define our version.
            version: None,
            // TODO: configure
            read_only: Some(true),
            // Only relevant in responses.
            requester_ip: None,
        }
    }

    /// Blocks until a response is received for a given transaction_id.
    /// Times out after the configured self.timeout.
    fn response(&mut self, transaction_id: u16) -> Result<Message> {
        // // TODO: Add timeout here!
        let (response_tx, response_rx) = mpsc::channel();
        self.outstanding_requests
            .lock()
            .unwrap()
            .insert(transaction_id, response_tx);
        let response = response_rx
            .recv()
            .map_err(|e| Error::Static("Failed to receive response>"))?;

        Ok(response)
    }

    fn ping(&mut self, address: SocketAddr) -> Result<Message> {
        let transaction_id = self.tid();
        let message = self.request_message(
            transaction_id,
            RequestSpecific::PingRequest(PingRequestArguments {
                requester_id: self.id,
            }),
        );

        // Send a message to the server.
        self.socket.send_to(&message.to_bytes()?, address)?;
        self.response(transaction_id)
    }

    fn find_node(&mut self, address: SocketAddr, target: Id) -> Result<Message> {
        let transaction_id = self.tid();
        let message = self.request_message(
            transaction_id,
            RequestSpecific::FindNodeRequest(FindNodeRequestArguments {
                target,
                requester_id: self.id,
            }),
        );

        // Send a message to the server.
        self.socket.send_to(&message.to_bytes()?, address)?;
        self.response(transaction_id)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_tid() {
        let mut rpc = Rpc::new().unwrap();

        assert_eq!(rpc.tid(), 0);
        assert_eq!(rpc.tid(), 1);
        assert_eq!(rpc.tid(), 2);

        rpc.next_tid = u16::max_value();

        assert_eq!(rpc.tid(), 65535);
        assert_eq!(rpc.tid(), 0);
    }

    #[test]
    fn test_recv_from() {
        let server = Rpc::new().unwrap();

        let server_clone = server.clone();
        thread::spawn(move || loop {
            if let Ok((message, from)) = server_clone.recv_from() {
                match message.message_type {
                    MessageType::Request(request_specific) => match request_specific {
                        RequestSpecific::PingRequest(args) => {
                            server_clone
                                .respond(
                                    message.transaction_id,
                                    from,
                                    ResponseSpecific::PingResponse(PingResponseArguments {
                                        responder_id: server_clone.id,
                                    }),
                                )
                                .unwrap();
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        });

        let server_addr = server.server_addr();

        // Start the client.
        let mut client = Rpc::new().unwrap();

        let client_clone = client.clone();
        thread::spawn(move || loop {
            client_clone.recv_from();
            // Do nothing ... responses will be sent to the outstanding_requests senders.
        });

        let pong = client.ping(server_addr).unwrap();
        let pong2 = client.ping(server_addr).unwrap();

        assert_eq!(pong.transaction_id, 0);
        assert_eq!(
            pong.message_type,
            MessageType::Response(ResponseSpecific::PingResponse(PingResponseArguments {
                responder_id: server.id
            }))
        );
        assert_eq!(
            pong.requester_ip.unwrap().port(),
            client.server_addr().port()
        );
        assert_eq!(pong2.transaction_id, 1);
    }

    // Live interoperability tests, should be removed before CI.
    #[test]
    fn test_live_ping() {
        let mut client = Rpc::new().unwrap();
        let client_clone = client.clone();
        thread::spawn(move || loop {
            client_clone.recv_from();
        });

        // TODO: resolve the address from DNS.
        let address: SocketAddr = "67.215.246.10:6881".parse().unwrap();

        client.ping(address);
    }

    // Live interoperability tests, should be removed before CI.
    #[test]
    fn test_live_find_node() {
        let mut client = Rpc::new().unwrap();
        let client_clone = client.clone();
        thread::spawn(move || loop {
            client_clone.recv_from();
        });

        // TODO: resolve the address from DNS.
        let address: SocketAddr = "67.215.246.10:6881".parse().unwrap();

        client.find_node(address, client.id);
    }
}
