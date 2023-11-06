//! Dht node.

use std::{
    net::{SocketAddr, UdpSocket},
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{rpc::Rpc, socket::KrpcSocket, Result};

const INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub struct Dht {
    handle: Option<JoinHandle<Result<()>>>,
    sender: Sender<ActorMessage>,
}

impl Clone for Dht {
    fn clone(&self) -> Self {
        Dht {
            handle: None,
            sender: self.sender.clone(),
        }
    }
}

impl Dht {
    pub fn new() -> Result<Self> {
        let (sender, receiver) = mpsc::channel();

        let mut dht = Dht {
            sender,
            handle: None,
        };

        let mut clone = dht.clone();

        let handle = thread::spawn(move || dht.run(receiver));

        clone.handle = Some(handle);

        Ok(clone)
    }

    // === Public Methods ===

    pub fn shutdown(&self) {
        self.sender.send(ActorMessage::Shutdown).unwrap();
    }

    // === Private Methods ===

    fn block_until_shutdown(self) {
        if let Some(handle) = self.handle {
            let _ = handle.join();
        }
    }

    fn run(&mut self, receiver: Receiver<ActorMessage>) -> Result<()> {
        let mut rpc = Rpc::new()?;

        rpc.populate();

        loop {
            if let Ok(actor_message) = receiver.try_recv() {
                match actor_message {
                    ActorMessage::Shutdown => {
                        break;
                    }
                }
            }

            if let Some((message, from)) = rpc.tick() {
                dbg!((message, from));
            };

            thread::sleep(INTERVAL)
        }

        Ok(())
    }
}

enum ActorMessage {
    Shutdown,
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use super::*;

    #[test]
    fn shutdown() {
        let dht = Dht::new().unwrap();

        let clone = dht.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(5000));

            clone.shutdown();
        });

        dht.block_until_shutdown();
    }
}
