use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::{Error, Result};

struct RPC {
    socket: UdpSocket,
    next_tid: u16,
}

impl RPC {
    fn new(port: u16) -> Result<RPC> {
        let address = SocketAddr::from(([0, 0, 0, 0], port));
        let socket = UdpSocket::bind(address)?;

        Ok(RPC {
            socket,
            next_tid: 0,
        })
    }

    fn listen(&self) -> JoinHandle<Result<()>> {
        let socket = self.socket.try_clone().expect("Failed to clone socket"); // Clone the socket to move into the thread.

        thread::spawn(move || -> Result<()> {
            loop {
                // // Buffer to hold incoming data.
                // let mut buf = [0u8; 1024];
                //
                // // Wait for a client to send data.
                // let (amt, src) = socket.recv_from(&mut buf)?;
                //
                // println!("Received in server: {:?}", &buf[0..amt]);
                //
                // // Echo the data back to the client.
                // socket.send_to(&buf[0..amt], &src)?;

                dbg!("inside loop");
                thread::sleep(Duration::from_secs(1));
            }

            Ok(())
        })
    }

    fn tid(&mut self) -> u16 {
        let tid = self.next_tid;
        self.next_tid = self.next_tid.wrapping_add(1);
        tid
    }

    fn ping(&mut self, address: SocketAddr) -> Result<()> {
        let tid = self.tid();
        dbg!(&tid);

        let msg = b"Hello, UDP Echo Server!";

        // Send a message to the server.
        self.socket.send_to(msg, address)?;

        // Wait for the server to echo the message back.
        let mut buf = [0u8; 1024];
        let (amt, _) = self.socket.recv_from(&mut buf)?;
        println!("Received in client: {:?}", &buf[0..amt]);

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_tid() {
        let mut rpc = RPC::new(12345).unwrap();

        assert_eq!(rpc.tid(), 0);
        assert_eq!(rpc.tid(), 1);
        assert_eq!(rpc.tid(), 2);

        rpc.next_tid = u16::max_value();

        assert_eq!(rpc.tid(), 65535);
        assert_eq!(rpc.tid(), 0);
    }

    // #[test]
    // fn test_rpc_echo() {
    //     let server = RPC::new(12345).unwrap();
    //     let listening = server.listen();
    //
    //     // Start the client.
    //     let client = RPC::new(12347).unwrap();
    //     client.ping(server.socket.local_addr().unwrap()).unwrap();
    //     client.ping(server.socket.local_addr().unwrap()).unwrap();
    //
    //     // listening.join().unwrap();
    // }
}
