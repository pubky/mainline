use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::{Error, Result};

// Define a UDP server struct
struct RPC {
    socket: UdpSocket,
}

impl RPC {
    // Create a new UDP server bound to the specified address and port
    fn new(bind_address: &str, port: u16) -> Result<Self> {
        let bind_str = format!("{}:{}", bind_address, port);
        let bind_addr: SocketAddr = bind_str.parse().unwrap();
        let socket = UdpSocket::bind(bind_addr)?;
        Ok(RPC { socket })
    }

    // Receive data from a client
    fn receive(&self) -> Result<(Vec<u8>, SocketAddr), std::io::Error> {
        let mut buf = vec![0; 1024];
        let (size, client_addr) = self.socket.recv_from(&mut buf)?;
        buf.resize(size, 0);
        Ok((buf, client_addr))
    }

    // Send data to a specific client
    fn ping(&self, data: &[u8], target: SocketAddr) -> Result<usize, std::io::Error> {
        self.socket.send_to(data, target)
    }

    fn handle(&self, data: &[u8], client_addr: SocketAddr) -> Result<Vec<u8>> {
        dbg!("GOT REQUEST HEREEEE");

        self.socket.send_to(b"Hello back!", client_addr)?;

        Ok(data.to_vec())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_ping() {
        // Create a UDP server and bind it to a specific address and port
        let server = RPC::new("127.0.0.1", 12345).unwrap();

        // Create a UDP client
        let client = RPC::new("127.0.0.1", 12346).unwrap();

        // Example: Sending data from client to server
        let message = b"Hello, UDP Server!";
        let server_addr = "127.0.0.1:12345".parse().unwrap();
        client.ping(message, server_addr).unwrap();

        // Example: Receiving data on the server and handling the method
        let (data, client_addr) = server.receive().unwrap();
        let response = server.handle(&data, client_addr).unwrap();

        println!(
            "Received data from client at {}: {:?}",
            client_addr, response
        );

        let (resres) = client.receive().unwrap();
        dbg!(resres);
    }
}
