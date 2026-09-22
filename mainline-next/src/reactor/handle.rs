use std::{
    fmt, io,
    net::{SocketAddr, SocketAddrV4},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use mio::net::UdpSocket;

use super::worker::{Worker, WorkerControl};

/// A cloneable handle to a library-owned IPv4 UDP reactor.
#[derive(Clone)]
pub struct ReactorHandle {
    local_addr: SocketAddrV4,
    control: Arc<ReactorControl>,
}

impl fmt::Debug for ReactorHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReactorHandle")
            .field("local_addr", &self.local_addr)
            .finish_non_exhaustive()
    }
}

impl ReactorHandle {
    /// Bind an IPv4 UDP socket and start a reactor on a library-owned thread.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if binding the socket or initializing the reactor fails.
    pub fn bind(address: SocketAddrV4) -> io::Result<Self> {
        Self::bind_inner(
            address,
            #[cfg(test)]
            None,
        )
    }

    fn bind_inner(
        address: SocketAddrV4,
        #[cfg(test)] exited: Option<std::sync::mpsc::SyncSender<()>>,
    ) -> io::Result<Self> {
        let socket = UdpSocket::bind(SocketAddr::V4(address))?;
        let local_addr = ipv4_address(socket.local_addr()?)?;
        let (worker, worker_control) = Worker::new(socket)?;

        let thread = thread::Builder::new()
            .name("mainline-reactor".to_owned())
            .spawn(move || {
                let result = worker.run();
                #[cfg(test)]
                if let Some(exited) = exited {
                    let _ = exited.send(());
                }
                result
            })?;

        Ok(Self {
            local_addr,
            control: Arc::new(ReactorControl::new(worker_control, thread)),
        })
    }

    /// Return the local IPv4 address bound by the reactor.
    pub fn local_addr(&self) -> SocketAddrV4 {
        self.local_addr
    }

    /// Stop this reactor for all handles and wait for its worker to exit.
    ///
    /// This blocks the calling thread until the worker has exited. In async
    /// applications, use the runtime's blocking facility to call this method.
    ///
    /// # Errors
    ///
    /// Returns an error if the worker's poll loop fails or panics.
    pub fn shutdown(self) -> Result<(), ShutdownError> {
        self.control.shutdown()
    }

    #[cfg(test)]
    fn bind_with_exit_signal(
        address: SocketAddrV4,
    ) -> io::Result<(Self, std::sync::mpsc::Receiver<()>)> {
        let (exited, receiver) = std::sync::mpsc::sync_channel(1);
        Self::bind_inner(address, Some(exited)).map(|reactor| (reactor, receiver))
    }
}

/// A failure reported while joining the reactor worker.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ShutdownError {
    /// The reactor stopped because of an I/O error.
    WorkerIo(Arc<io::Error>),
    /// The reactor thread panicked.
    WorkerPanicked,
}

impl fmt::Display for ShutdownError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkerIo(error) => write!(formatter, "reactor worker failed: {error}"),
            Self::WorkerPanicked => formatter.write_str("reactor worker panicked"),
        }
    }
}

impl std::error::Error for ShutdownError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::WorkerIo(error) => Some(error.as_ref()),
            Self::WorkerPanicked => None,
        }
    }
}

struct ReactorControl {
    worker_control: WorkerControl,
    join: Mutex<JoinState>,
}

impl ReactorControl {
    fn new(worker_control: WorkerControl, thread: JoinHandle<io::Result<()>>) -> Self {
        Self {
            worker_control,
            join: Mutex::new(JoinState::Unjoined(thread)),
        }
    }

    fn shutdown(&self) -> Result<(), ShutdownError> {
        self.worker_control.request_shutdown();
        // Hold the lock through joining so concurrent callers share the cached result.
        self.join
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .join()
    }
}

impl Drop for ReactorControl {
    fn drop(&mut self) {
        self.worker_control.request_shutdown();
    }
}

enum JoinState {
    Unjoined(JoinHandle<io::Result<()>>),
    Joined(Result<(), ShutdownError>),
}

impl JoinState {
    fn join(&mut self) -> Result<(), ShutdownError> {
        let state = std::mem::replace(self, Self::Joined(Ok(())));
        let result = match state {
            Self::Unjoined(thread) => match thread.join() {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(ShutdownError::WorkerIo(Arc::new(error))),
                Err(_) => Err(ShutdownError::WorkerPanicked),
            },
            Self::Joined(result) => result,
        };
        *self = Self::Joined(result.clone());
        result
    }
}

fn ipv4_address(address: SocketAddr) -> io::Result<SocketAddrV4> {
    match address {
        SocketAddr::V4(address) => Ok(address),
        SocketAddr::V6(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPv4 socket returned an IPv6 address",
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        net::{Ipv4Addr, UdpSocket as StdUdpSocket},
        sync::Barrier,
        time::Duration,
    };

    use super::*;

    const WAIT_TIMEOUT: Duration = Duration::from_secs(3);

    fn loopback() -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)
    }

    fn assert_port_is_occupied(address: SocketAddrV4) {
        let error = StdUdpSocket::bind(address).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AddrInUse);
    }

    #[test]
    fn binds_an_ephemeral_ipv4_address() {
        let reactor = ReactorHandle::bind(loopback()).unwrap();

        assert_eq!(*reactor.local_addr().ip(), Ipv4Addr::LOCALHOST);
        assert_ne!(reactor.local_addr().port(), 0);
        reactor.shutdown().unwrap();
    }

    #[test]
    fn reports_an_occupied_address() {
        let occupied = StdUdpSocket::bind(loopback()).unwrap();
        let address = occupied.local_addr().unwrap();
        let SocketAddr::V4(address) = address else {
            panic!("expected an IPv4 address");
        };

        let error = match ReactorHandle::bind(address) {
            Ok(_) => panic!("expected binding an occupied address to fail"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), io::ErrorKind::AddrInUse);
    }

    #[test]
    fn dropping_one_clone_keeps_the_socket_bound() {
        let reactor = ReactorHandle::bind(loopback()).unwrap();
        let clone = reactor.clone();
        let address = reactor.local_addr();

        drop(reactor);

        assert_port_is_occupied(address);
        clone.shutdown().unwrap();
    }

    #[test]
    fn explicit_shutdown_wakes_the_worker_and_releases_the_socket() {
        let reactor = ReactorHandle::bind(loopback()).unwrap();
        let surviving_clone = reactor.clone();
        let address = reactor.local_addr();

        reactor.shutdown().unwrap();

        StdUdpSocket::bind(address).unwrap();
        surviving_clone.shutdown().unwrap();
    }

    #[test]
    fn dropping_the_final_handle_releases_the_worker_and_socket() {
        let (reactor, exited) = ReactorHandle::bind_with_exit_signal(loopback()).unwrap();
        let address = reactor.local_addr();

        drop(reactor);

        assert_eq!(exited.recv_timeout(WAIT_TIMEOUT), Ok(()));
        StdUdpSocket::bind(address).unwrap();
    }

    #[test]
    fn concurrent_shutdown_calls_share_the_worker_result() {
        let reactor = ReactorHandle::bind(loopback()).unwrap();
        let clone = reactor.clone();
        let barrier = Arc::new(Barrier::new(3));
        let first_barrier = Arc::clone(&barrier);
        let second_barrier = Arc::clone(&barrier);

        let first = thread::spawn(move || {
            first_barrier.wait();
            reactor.shutdown()
        });
        let second = thread::spawn(move || {
            second_barrier.wait();
            clone.shutdown()
        });
        barrier.wait();

        assert!(first.join().unwrap().is_ok());
        assert!(second.join().unwrap().is_ok());
    }

    fn join_state_with_worker_outcome(outcome: fn() -> io::Result<()>) -> JoinState {
        JoinState::Unjoined(thread::spawn(outcome))
    }

    #[test]
    fn join_state_caches_worker_io_failure() {
        let mut join =
            join_state_with_worker_outcome(|| Err(io::Error::other("simulated poll failure")));

        let ShutdownError::WorkerIo(first) = join.join().unwrap_err() else {
            panic!("expected worker I/O failure");
        };
        let ShutdownError::WorkerIo(second) = join.join().unwrap_err() else {
            panic!("expected cached worker I/O failure");
        };
        assert_eq!(first.kind(), io::ErrorKind::Other);
        assert_eq!(first.to_string(), "simulated poll failure");
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn join_state_caches_worker_panic() {
        let mut join = join_state_with_worker_outcome(|| panic!("simulated worker panic"));

        assert!(matches!(join.join(), Err(ShutdownError::WorkerPanicked)));
        assert!(matches!(join.join(), Err(ShutdownError::WorkerPanicked)));
    }
}
