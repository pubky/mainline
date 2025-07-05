use std::fmt::Debug;
use std::io;
use std::net::SocketAddr;
use std::time::Duration;

pub trait Udp: Debug + Send {
    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)>;
    fn send_to(&self, buf: &[u8], addr: SocketAddr) -> io::Result<usize>;
    fn local_addr(&self) -> io::Result<SocketAddr>;
    fn set_read_timeout(&mut self, dur: Option<Duration>) -> io::Result<()>;
}

pub mod real {
    use super::*;

    #[derive(Debug)]
    pub struct UdpSocket(pub(crate) std::net::UdpSocket);

    impl UdpSocket {
        pub fn bind(addr: SocketAddr) -> io::Result<Box<Self>> {
            Ok(Box::new(Self(std::net::UdpSocket::bind(addr)?)))
        }
    }

    impl Udp for UdpSocket {
        fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
            self.0.recv_from(buf)
        }
        fn send_to(&self, buf: &[u8], addr: SocketAddr) -> io::Result<usize> {
            self.0.send_to(buf, addr)
        }
        fn local_addr(&self) -> io::Result<SocketAddr> {
            self.0.local_addr()
        }
        fn set_read_timeout(&mut self, dur: Option<Duration>) -> io::Result<()> {
            self.0.set_read_timeout(dur)?;
            Ok(())
        }
    }
}

pub mod sim {
    pub use super::*;

    use std::collections::HashMap;
    use std::net::Ipv4Addr;
    use std::sync::{Mutex, OnceLock};

    type ChannelMessage = (Box<[u8]>, SocketAddr);

    static CHANNELS: OnceLock<std::sync::Mutex<HashMap<SocketAddr, Vec<ChannelMessage>>>> =
        OnceLock::new();

    fn get_channels() -> &'static Mutex<HashMap<SocketAddr, Vec<ChannelMessage>>> {
        CHANNELS.get_or_init(|| Mutex::new(Default::default()))
    }

    #[derive(Debug)]
    pub struct UdpSocket(SocketAddr);

    impl UdpSocket {
        pub fn bind() -> io::Result<Box<Self>> {
            let mut bytes: [u8; 6] = [0; 6];
            getrandom::getrandom(&mut bytes).expect("getrandom");
            let local_addr = SocketAddr::from((
                Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]),
                u16::from_be_bytes(bytes[4..].try_into().unwrap()),
            ));

            get_channels()
                .lock()
                .expect("mutex")
                .insert(local_addr, vec![]);

            Ok(Box::new(Self(local_addr)))
        }
    }

    impl Drop for UdpSocket {
        fn drop(&mut self) {
            let mut channels = get_channels().lock().expect("mutex");
            channels.remove(&self.0);
        }
    }

    impl Udp for UdpSocket {
        fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
            let mut channels = get_channels().lock().expect("mutex");
            let channel = channels
                .get_mut(&self.0)
                .expect("udp::sim::UdpSocket messages queue dropped unexpectedly");

            if let Some((message, from)) = channel.pop() {
                let size = message.len();
                buf[0..size].copy_from_slice(&message);

                Ok((size, from))
            } else {
                Err(io::Error::other("udp::sim::UdpSocket recv timeout"))
            }
        }
        fn send_to(&self, buf: &[u8], addr: SocketAddr) -> io::Result<usize> {
            let size = buf.len();

            let mut channels = get_channels().lock().expect("mutex");
            if let Some(channel) = channels.get_mut(&addr) {
                channel.push((buf.to_vec().into_boxed_slice(), self.0));
            } else {
                // UDP packet sent to the void.
            }

            Ok(size)
        }
        fn local_addr(&self) -> io::Result<SocketAddr> {
            Ok(self.0)
        }
        fn set_read_timeout(&mut self, _dur: Option<Duration>) -> io::Result<()> {
            Ok(())
        }
    }
}
