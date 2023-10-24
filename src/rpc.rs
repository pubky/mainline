use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::common::Id;
use crate::messages::{
    Message, MessageType, PingRequestArguments, PingResponseArguments, RequestSpecific,
    ResponseSpecific,
};
use crate::{Error, Result};

const DEFAULT_PORT: u16 = 6881;

struct RPC {
    id: Id,
    socket: UdpSocket,
    next_tid: u16,
    sender: Sender<ListenerMessage>,
    /// Hande to the server thread.
    server_handle: JoinHandle<()>,
    // Request tid => response message.
    outstanding_requests: Arc<RwLock<BTreeSet<u16>>>,
    responses: Arc<RwLock<BTreeMap<u16, Option<Message>>>>,
}

#[derive(Debug)]
enum ListenerMessage {
    Shutdown,
    RegisterRequest((u16, Sender<Message>)),
}

impl RPC {
    fn new() -> Result<RPC> {
        // TODO: One day I might implement BEP42.
        let id = Id::random();

        let socket = match UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT))) {
            Ok(socket) => Ok(socket),
            Err(_) => UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))),
        }?;

        socket.set_read_timeout(Some(Duration::from_millis(100)))?;

        // Create a channel to wait for the response.
        let (tx, rx) = mpsc::channel::<ListenerMessage>();

        let cloned_socket = socket.try_clone()?;
        let cloned_id = id.clone();

        let server_handle = thread::spawn(move || listen(cloned_id, cloned_socket, rx));

        Ok(RPC {
            id,
            socket,
            next_tid: 0,
            sender: tx,
            server_handle,
            outstanding_requests: Arc::new(RwLock::new(BTreeSet::new())),
            responses: Arc::new(RwLock::new(BTreeMap::new())),
        })
    }

    fn tid(&mut self) -> u16 {
        let tid = self.next_tid;
        self.next_tid = self.next_tid.wrapping_add(1);
        tid
    }

    /// Create a new request message with the given request specific arguments.
    fn request_message(&mut self, tid: u16, request: RequestSpecific) -> Message {
        Message {
            transaction_id: tid.to_be_bytes().into(),
            message_type: MessageType::Request(request),
            // TODO: define our version.
            version: None,
            read_only: None,
            // Only relevant in responses.
            requester_ip: None,
        }
    }

    fn ping(&mut self, address: SocketAddr) -> Result<Message> {
        let tid = self.tid();
        let message = self
            .request_message(
                tid,
                RequestSpecific::PingRequest(PingRequestArguments {
                    requester_id: self.id,
                }),
            )
            .to_bytes()?;

        let (response_tx, response_rx) = mpsc::channel();

        self.sender
            .send(ListenerMessage::RegisterRequest((tid, response_tx)));

        // Send a message to the server.
        self.socket.send_to(&message, address)?;

        // TODO: Add timeout here!
        let response = response_rx
            .recv()
            .map_err(|e| Error::Static("Failed to receive response>"))?;

        Ok(response)
    }

    fn shutdown(&self) -> Result<()> {
        self.sender
            .send(ListenerMessage::Shutdown)
            .map_err(|_| Error::Static("Failed to send shutdown message to server thread."))?;

        Ok(())
    }

    fn block_until_shutdown(self) {
        self.server_handle.join().unwrap();
    }
}

fn listen(id: Id, socket: UdpSocket, rx: Receiver<ListenerMessage>) -> () {
    // // Buffer to hold incoming data.
    let mut buf = [0u8; 1024];
    // TODO: timeout clean requests probably with ListenerMessage::Timeout(tid);
    let mut requests = BTreeMap::<u16, Sender<Message>>::new();
    let mut responses = BTreeMap::<u16, Message>::new();

    loop {
        match rx.try_recv() {
            Ok(ListenerMessage::Shutdown) => {
                break;
            }
            Ok(ListenerMessage::RegisterRequest((tid, sender))) => {
                &requests.insert(tid, sender);
            }
            _ => {}
        };

        let mut responses_to_remove: Vec<u16> = vec![];

        // Match request/response.
        for (tid, response) in responses.iter() {
            match &requests.remove(&tid) {
                Some(request) => {
                    responses_to_remove.push(*tid);
                    request.send(response.clone()).unwrap();
                }
                None => {}
            }
        }

        // Clean responses.
        responses_to_remove.iter().for_each(|tid| {
            responses.remove(tid);
        });

        match socket.recv_from(&mut buf) {
            Ok((amt, requester)) => {
                let mut msg = match Message::from_bytes(&buf[..amt]) {
                    Ok(msg) => msg,
                    Err(_) => {
                        // TODO: tracing
                        println!("Failed to parse message from {:?}", requester);
                        continue;
                    }
                };

                match &msg.message_type {
                    MessageType::Request(request_type) => {
                        // TODO: check if it is IPV4 or IPV6
                        // TODO: support IPV6
                        msg.requester_ip = Some(requester);

                        println!("Received a request: {:?}", &msg);

                        let response = match request_type {
                            RequestSpecific::PingRequest(ping_request_arguments) => {
                                Message {
                                    transaction_id: msg.transaction_id.clone(),
                                    requester_ip: Some(requester),
                                    message_type: MessageType::Response(
                                        ResponseSpecific::PingResponse(PingResponseArguments {
                                            responder_id: id,
                                        }),
                                    ),
                                    // TODO: define these variables
                                    version: None,
                                    read_only: None,
                                }
                            }
                        };

                        socket
                            .send_to(&response.to_bytes().unwrap(), requester)
                            .unwrap();
                    }
                    MessageType::Response(_) => {
                        let tid = match msg.transaction_id() {
                            Ok(tid) => tid,
                            Err(_) => {
                                // TODO: tracing
                                println!("Failed to parse response message transaction_id, expected 2 bytes {:?}", msg);
                                continue;
                            }
                        };

                        println!("Received a response: {:?}", &msg);

                        &responses.insert(tid, msg.clone());
                    }
                    MessageType::Error(_) => {
                        println!("Received an error: {:?}", &msg);
                    }
                }
            }
            Err(_) => {}
        }
    }

    ()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_tid() {
        let mut rpc = RPC::new().unwrap();

        assert_eq!(rpc.tid(), 0);
        assert_eq!(rpc.tid(), 1);
        assert_eq!(rpc.tid(), 2);

        rpc.next_tid = u16::max_value();

        assert_eq!(rpc.tid(), 65535);
        assert_eq!(rpc.tid(), 0);
    }

    #[test]
    fn test_ping() {
        let server = RPC::new().unwrap();

        let server_addr = server.socket.local_addr().unwrap();
        // Start the client.
        let mut client = RPC::new().unwrap();
        let a = client.ping(server_addr).unwrap();
        let b = client.ping(server_addr).unwrap();
        let c = client.ping(server_addr).unwrap();

        dbg!((a, b, c));
        // TODO: prove that we gced everything
    }

    // Live interoperability tests, should be removed before CI.
    #[test]
    fn test_live_ping() {
        let mut client = RPC::new().unwrap();

        // TODO: resolve the address from DNS.
        let address: SocketAddr = "167.86.102.121:6881".parse().unwrap();

        client.ping(address);
    }
}
