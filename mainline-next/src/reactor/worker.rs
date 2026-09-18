use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use mio::{net::UdpSocket, Events, Interest, Poll, Token, Waker};
use tracing::trace;

const WAKE_TOKEN: Token = Token(0);
const SOCKET_TOKEN: Token = Token(1);
const SHUTDOWN_FALLBACK: Duration = Duration::from_secs(1);

pub(super) struct Worker {
    poll: Poll,
    events: Events,
    _socket: UdpSocket,
    shutdown_requested: Arc<AtomicBool>,
}

impl Worker {
    pub(super) fn new(mut socket: UdpSocket) -> io::Result<(Self, WorkerControl)> {
        let poll = Poll::new()?;
        poll.registry()
            .register(&mut socket, SOCKET_TOKEN, Interest::READABLE)?;
        let waker = Waker::new(poll.registry(), WAKE_TOKEN)?;
        let shutdown_requested = Arc::new(AtomicBool::new(false));

        Ok((
            Self {
                poll,
                events: Events::with_capacity(1),
                _socket: socket,
                shutdown_requested: Arc::clone(&shutdown_requested),
            },
            WorkerControl {
                shutdown_requested,
                waker,
            },
        ))
    }

    pub(super) fn run(mut self) -> io::Result<()> {
        while !self.shutdown_requested.load(Ordering::Acquire) {
            match self.poll.poll(&mut self.events, Some(SHUTDOWN_FALLBACK)) {
                Ok(()) => {
                    for event in &self.events {
                        match event.token() {
                            WAKE_TOKEN => trace!("reactor wake event"),
                            SOCKET_TOKEN => trace!("reactor socket event"),
                            _token => trace!(?event, "reactor event"),
                        }
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

pub(super) struct WorkerControl {
    shutdown_requested: Arc<AtomicBool>,
    waker: Waker,
}

impl WorkerControl {
    pub(super) fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::Release);
        // Worker::run uses a bounded poll timeout, so shutdown still completes
        // if waking fails.
        let _ = self.waker.wake();
    }
}
