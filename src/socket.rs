use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use crate::messages::{ErrorSpecific, Message, MessageType, RequestSpecific, ResponseSpecific};

use crate::Result;

const DEFAULT_PORT: u16 = 6881;
const VERSION: &[u8] = "RS".as_bytes(); // The Mainline rust implementation.
const MTU: usize = 2048;

/// A UdpSocket wrapper that manages inflight requests and receive incoming messages.
#[derive(Debug)]
pub struct KrpcSocket {
    next_tid: u16,
    socket: UdpSocket,
    read_only: bool,
}

impl Clone for KrpcSocket {
    fn clone(&self) -> Self {
        Self {
            socket: self.socket.try_clone().unwrap(),
            next_tid: self.next_tid,
            read_only: self.read_only,
        }
    }
}

impl KrpcSocket {
    pub fn new() -> Result<Self> {
        let socket = match UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT))) {
            Ok(socket) => Ok(socket),
            Err(_) => UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))),
        }?;
        socket.set_nonblocking(true)?;

        Ok(Self {
            socket,
            next_tid: 0,
            read_only: false,
        })
    }

    // === Options ===

    /// Set read-only mode
    pub fn with_read_only(self, read_only: bool) -> Self {
        Self { read_only, ..self }
    }

    /// Set the starting transaciton_id mostly for testing purposes.
    fn with_tid(self, tid: u16) -> Self {
        Self {
            next_tid: tid,
            ..self
        }
    }

    /// Returns the address the server is listening to.
    #[inline]
    pub fn local_addr(&self) -> SocketAddr {
        self.socket.local_addr().unwrap()
    }

    // === Public Methods ===

    /// Send a request to the given address and return a receiver for the response.
    pub fn request(&mut self, address: SocketAddr, request: &RequestSpecific) {
        let message = self.request_message(request.clone());
        self.send(address, message);
    }

    /// Send a response to the given address.
    pub fn response(
        &mut self,
        address: SocketAddr,
        transaction_id: u16,
        response: ResponseSpecific,
    ) -> Result<()> {
        let message =
            self.response_message(MessageType::Response(response), address, transaction_id);
        self.send(address, message)
    }

    /// Send an error to the given address.
    pub fn error(
        &mut self,
        address: SocketAddr,
        transaction_id: u16,
        error: ErrorSpecific,
    ) -> Result<()> {
        let message = self.response_message(MessageType::Error(error), address, transaction_id);
        self.send(address, message)
    }

    /// Receives a single krpc message on the socket.
    /// On success, returns the dht message and the origin.
    pub fn recv_from(&mut self) -> Option<(Message, SocketAddr)> {
        let mut buf = [0u8; MTU];

        if let Ok((amt, from)) = self.socket.recv_from(&mut buf) {
            match Message::from_bytes(&buf[..amt]) {
                Ok(message) => {
                    return Some((message, from));
                }
                Err(err) => {
                    println!("Error parsing incoming message from {:?}: {:?}", from, err);
                }
            };
        };

        None
    }

    // === Private Methods ===

    /// Increments self.next_tid and returns the previous value.
    fn tid(&mut self) -> u16 {
        let tid = self.next_tid;
        self.next_tid = self.next_tid.wrapping_add(1);
        tid
    }

    /// Set transactin_id, version and read_only
    fn request_message(&mut self, message: RequestSpecific) -> Message {
        let transaction_id = self.tid();

        Message {
            transaction_id,
            message_type: MessageType::Request(message),
            version: Some(VERSION.into()),
            read_only: self.read_only,
            requester_ip: None,
        }
    }

    /// Same as request_message but with request transaction_id and the requester_ip.
    fn response_message(
        &mut self,
        message: MessageType,
        requester_ip: SocketAddr,
        request_tid: u16,
    ) -> Message {
        Message {
            transaction_id: request_tid,
            message_type: message,
            version: Some(VERSION.into()),
            read_only: self.read_only,
            // BEP0042 Only relevant in responses.
            requester_ip: Some(requester_ip),
        }
    }

    /// Send a raw dht message
    fn send(&mut self, address: SocketAddr, message: Message) -> Result<()> {
        self.socket.send_to(&message.to_bytes()?, address)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::thread;

    use crate::{
        common::Id,
        messages::{PingRequestArguments, PingResponseArguments},
    };

    use super::*;

    #[test]
    fn tid() {
        let mut socket = KrpcSocket::new().unwrap();

        assert_eq!(socket.tid(), 0);
        assert_eq!(socket.tid(), 1);
        assert_eq!(socket.tid(), 2);

        socket.next_tid = u16::max_value();

        assert_eq!(socket.tid(), 65535);
        assert_eq!(socket.tid(), 0);
    }

    #[test]
    fn recv_request() {
        let mut server = KrpcSocket::new().unwrap();
        let server_address = server.local_addr();

        let mut client = KrpcSocket::new()
            .unwrap()
            .with_tid(120)
            .with_read_only(true);

        let client_address = client.local_addr();
        let request = RequestSpecific::PingRequest(PingRequestArguments {
            requester_id: Id::random(),
        });

        let expected_request = request.clone();

        let server_loop = thread::spawn(move || loop {
            if let Some((message, from)) = server.recv_from() {
                assert_eq!(from.port(), client_address.port());
                assert_eq!(message.transaction_id, 120);
                assert_eq!(message.read_only, true, "Read-only should be true");
                assert_eq!(
                    message.version,
                    Some(VERSION.to_vec()),
                    "Version should be 'RS'"
                );
                assert_eq!(message.message_type, MessageType::Request(expected_request));
                break;
            }
        });

        client.request(server_address, &request);

        server_loop.join().unwrap();
    }
}
