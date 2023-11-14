//! Dht node.

use std::{
    net::SocketAddr,
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

use crate::{
    common::{GetPeerResponse, Id, Node, Response, ResponseDone, ResponseMessage, ResponseSender},
    rpc::Rpc,
    Result,
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
        self.sender.send(ActorMessage::Shutdown).unwrap();
    }

    pub fn get_peers(&self, info_hash: Id) -> Response<GetPeerResponse> {
        let (sender, receiver) = mpsc::channel::<ResponseMessage<GetPeerResponse>>();

        let _ = self.sender.send(ActorMessage::GetPeers(info_hash, sender));

        Response::new(receiver)
    }

    pub fn announce_peer(&self, info_hash: Id, port: u16) {
        // let (sender, receiver) = mpsc::channel::<ResponseMessage<GetPeerResponse>>();
        //
        // let _ = self
        //     .sender
        //     .send(ActorMessage::AnnouncePeer(info_hash, sender));
        //
        // let result = Response::new(receiver);
        //
        // // let
        // //
        // // for response in result {}
        // //
        // // result
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
                    ActorMessage::AnnouncePeer(info_hash, sender) => {
                        todo!();
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
    AnnouncePeer(Id, Sender<ResponseMessage<GetPeerResponse>>),
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
