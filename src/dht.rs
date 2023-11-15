//! Dht node.

use std::{
    net::SocketAddr,
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

use crate::{
    common::{
        GetPeerResponse, Id, Node, Response, ResponseDone, ResponseMessage, ResponseSender,
        ResponseValue, StoreResponse,
    },
    rpc::Rpc,
    Error, Result,
};

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
        let _ = self.sender.send(ActorMessage::Shutdown);
    }

    /// Get peers for a given infohash.
    ///
    /// Returns an blocking iterator over responses as they are received.
    pub fn get_peers(&self, info_hash: Id) -> Response<GetPeerResponse> {
        let (sender, receiver) = mpsc::channel::<ResponseMessage<GetPeerResponse>>();

        let _ = self.sender.send(ActorMessage::GetPeers(info_hash, sender));

        Response::new(receiver)
    }

    /// Announce a peer for a given infohash.
    ///
    /// The peer will be announced on this process IP.
    /// If explicit port is passed, it will be used, otherwise the port will be implicitly
    /// assumed by remote nodes to be the same ase port they recieved the request from.
    pub fn announce_peer(&self, info_hash: Id, port: Option<u16>) -> Result<StoreResponse> {
        let (sender, receiver) = mpsc::channel::<ResponseMessage<GetPeerResponse>>();

        let _ = self.sender.send(ActorMessage::GetPeers(info_hash, sender));

        let mut response = Response::new(receiver);

        // Block until we got a Done response!
        for value in &mut response {}

        self.announce_peer_to(info_hash, response.closest_nodes, port)
    }

    pub fn announce_peer_to(
        &self,
        info_hash: Id,
        nodes: Vec<Node>,
        port: Option<u16>,
    ) -> Result<StoreResponse> {
        let (sender, receiver) = mpsc::channel::<StoreResponse>();

        let _ = self
            .sender
            .send(ActorMessage::AnnouncePeer(info_hash, nodes, port, sender));

        receiver.recv().map_err(|e| e.into())
    }

    // === Private Methods ===

    #[cfg(test)]
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
                        rpc.get_peers(info_hash, ResponseSender::GetPeer(sender))
                    }
                    ActorMessage::AnnouncePeer(info_hash, nodes, port, sender) => {
                        rpc.announce_peer(info_hash, nodes, port, ResponseSender::StoreItem(sender))
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
    GetPeers(Id, Sender<ResponseMessage<GetPeerResponse>>),
    AnnouncePeer(Id, Vec<Node>, Option<u16>, Sender<StoreResponse>),
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
}
