//! Dht node.

use std::{
    net::SocketAddr,
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

use crate::{common::Id, rpc::Rpc, Result};

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

    pub fn get_peers(&self, info_hash: Id) -> Response<SocketAddr> {
        let (sender, receiver) = mpsc::channel::<Option<SocketAddr>>();

        let _ = self.sender.send(ActorMessage::GetPeers(info_hash, sender));

        Response { receiver }
    }

    // === Private Methods ===

    fn block_until_shutdown(self) {
        if let Some(handle) = self.handle {
            let _ = handle.join();
        }
    }

    fn run(&mut self, receiver: Receiver<ActorMessage>) -> Result<()> {
        // TODO: pass config
        let mut rpc = Rpc::new()?.with_read_only(true);

        loop {
            if let Ok(actor_message) = receiver.try_recv() {
                match actor_message {
                    ActorMessage::Shutdown => {
                        break;
                    }
                    ActorMessage::GetPeers(info_hash, sender) => {
                        rpc.get_peers(info_hash, ResponseSender::Peer(sender))
                    }
                }
            }

            rpc.tick();
        }

        Ok(())
    }
}

enum ActorMessage {
    Shutdown,
    GetPeers(Id, Sender<Option<SocketAddr>>),
}

pub struct Response<ResponseItem> {
    receiver: Receiver<Option<ResponseItem>>,
}

#[derive(Debug)]
pub enum ResponseSender {
    Peer(Sender<Option<SocketAddr>>),
}

pub enum ResponseItem {
    Peer(SocketAddr),
}

impl Iterator for Response<SocketAddr> {
    type Item = SocketAddr;

    fn next(&mut self) -> Option<Self::Item> {
        if let Ok(item) = self.receiver.recv() {
            return item;
        }
        None
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryInto;
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn shutdown() {
        let dht = Dht::new().unwrap();

        let clone = dht.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));

            clone.shutdown();
        });

        dht.block_until_shutdown();
    }

    // Live tests that shouldn't run in CI etc.

    #[test]
    fn live_get_peers() {
        let dht = Dht::new().unwrap();

        let info_hash: Id = "c87a1b64bf00a072cc937688908d1be4f7ad2489"
            .try_into()
            .unwrap();

        let response = dht.get_peers(info_hash);

        let instant = Instant::now();
        for peer in response {
            println!("\nFound Peer: {:?}", peer);
            println!("Took {:?}\n", instant.elapsed());
            break;
        }
    }
}
