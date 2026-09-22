use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
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
    // Keep the registration alive until the poll loop has exited. In
    // particular, the final ReactorHandle may drop immediately after waking us.
    waker: Arc<Waker>,
    #[cfg(test)]
    poll_started: Option<std::sync::mpsc::SyncSender<()>>,
}

impl Worker {
    pub(super) fn new(
        mut socket: UdpSocket,
        #[cfg(test)] poll_started: Option<std::sync::mpsc::SyncSender<()>>,
    ) -> io::Result<Self> {
        let poll = Poll::new()?;
        poll.registry()
            .register(&mut socket, SOCKET_TOKEN, Interest::READABLE)?;
        let waker = Arc::new(Waker::new(poll.registry(), WAKE_TOKEN)?);
        let shutdown_requested = Arc::new(AtomicBool::new(false));

        Ok(Self {
            poll,
            events: Events::with_capacity(1),
            _socket: socket,
            shutdown_requested,
            waker,
            #[cfg(test)]
            poll_started,
        })
    }

    pub(super) fn control(&self) -> WorkerControl {
        WorkerControl {
            shutdown_requested: Arc::clone(&self.shutdown_requested),
            waker: Arc::downgrade(&self.waker),
        }
    }

    pub(super) fn run(mut self) -> io::Result<()> {
        while !self.shutdown_requested.load(Ordering::Acquire) {
            #[cfg(test)]
            if let Some(poll_started) = self.poll_started.take() {
                let _ = poll_started.send(());
            }
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
    waker: Weak<Waker>,
}

impl WorkerControl {
    pub(super) fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::Release);
        // Worker::run uses a bounded poll timeout, so shutdown still completes
        // if the worker has exited or waking fails.
        if let Some(waker) = self.waker.upgrade() {
            let _ = waker.wake();
        }
    }
}
