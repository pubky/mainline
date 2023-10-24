use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::common::Id;
use crate::messages::{Message, MessageType, PingRequestArguments, RequestSpecific};
use crate::{Error, Result};

const DEFAULT_PORT: u16 = 6881;

struct RPC {
    id: Id,
    socket: UdpSocket,
    next_tid: u16,
    sender: Sender<ActorMessage>,
    /// Hande to the server thread.
    server_handle: JoinHandle<Result<()>>,
}

#[derive(Debug)]
enum ActorMessage {
    Shutdown,
}

impl RPC {
    fn new() -> Result<RPC> {
        let socket = match UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT))) {
            Ok(socket) => Ok(socket),
            Err(_) => UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))),
        }?;

        socket.set_read_timeout(Some(Duration::from_millis(100)))?;

        // Create a channel to wait for the response.
        let (tx, rx) = mpsc::channel::<ActorMessage>();

        let cloned_socket = socket.try_clone()?;

        let server_handle = thread::spawn(|| listen(cloned_socket, rx));

        Ok(RPC {
            id: Id::random(),
            socket,
            next_tid: 0,
            sender: tx,
            server_handle,
        })
    }

    fn tid(&mut self) -> u16 {
        let tid = self.next_tid;
        self.next_tid = self.next_tid.wrapping_add(1);
        tid
    }

    /// Create a new request message with the given request specific arguments.
    fn request_message(&mut self, request: RequestSpecific) -> Message {
        let tid = self.tid().to_be_bytes();

        Message {
            transaction_id: tid.into(),
            message_type: MessageType::Request(request),
            // TODO: define our version.
            version: None,
            // TODO: Our ip address, do we know it yet?
            requester_ip: None,
            read_only: None,
        }
    }

    fn ping(&mut self, address: SocketAddr) -> Result<()> {
        let message = self
            .request_message(RequestSpecific::PingRequest(PingRequestArguments {
                requester_id: self.id,
            }))
            .to_bytes()?;

        // Send a message to the server.
        self.socket.send_to(&message, address)?;

        // let response = response_rx.recv().unwrap();
        //
        // dbg!(response);

        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        self.sender
            .send(ActorMessage::Shutdown)
            .map_err(|_| Error::Static("Failed to send shutdown message to server thread."))?;

        Ok(())
    }

    fn block_until_shutdown(self) {
        self.server_handle.join().unwrap();
    }
}

fn listen(socket: UdpSocket, rx: Receiver<ActorMessage>) -> Result<()> {
    // // Buffer to hold incoming data.
    let mut buf = [0u8; 1024];

    loop {
        match rx.recv_timeout(Duration::from_micros(1)) {
            Ok(ActorMessage::Shutdown) => {
                break;
            }
            _ => {}
        };

        match socket.recv_from(&mut buf) {
            Ok((_, requester)) => {
                let mut msg = Message::from_bytes(buf)?;

                // TODO: check if it is IPV4 or IPV6
                // TODO: support IPV6
                msg.requester_ip = Some(requester);

                println!("Received in server: {:?}", &msg);
            }
            Err(_) => {}
        }
    }

    Ok(())
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
    fn test_rpc_echo() {
        let server = RPC::new().unwrap();

        let server_addr = server.socket.local_addr().unwrap();
        // Start the client.
        let mut client = RPC::new().unwrap();
        client.ping(server_addr).unwrap();
        client.ping(server_addr).unwrap();
        client.ping(server_addr).unwrap();

        thread::sleep(Duration::from_secs(1));
        server.shutdown();

        server.block_until_shutdown();
    }

    // Live interoperability tests, should be removed before CI.
    #[test]
    fn test_live_ping() {
        let mut client = RPC::new().unwrap();

        // TODO: resolve the address from DNS.
        let address: SocketAddr = "167.86.102.121:6881".parse().unwrap();
        dbg!(&address);

        client.ping(address);
        thread::sleep(Duration::from_secs(1));
    }
}
