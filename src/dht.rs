//! Dht node.

use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

use crate::Result;

#[derive(Debug)]
pub struct Dht {
    sender: Sender<ActorMessage>,
    handle: Option<JoinHandle<()>>,
}

impl Clone for Dht {
    fn clone(&self) -> Self {
        Dht {
            sender: self.sender.clone(),
            handle: None,
        }
    }
}

impl Dht {
    pub fn new() -> Result<Self> {
        // let mut rpc = Rpc::new().unwrap();
        let (sender, receiver) = mpsc::channel();

        // let rpc = Rpc::new()?;
        let handle = thread::spawn(|| run(receiver));

        Ok(Dht {
            sender,
            handle: Some(handle),
        })
    }

    pub fn shutdown(&self) {
        self.sender.send(ActorMessage::Shutdown).unwrap();
    }

    pub fn block_until_shutdown(self) {
        if let Some(handle) = self.handle {
            let _ = handle.join();
        }
    }
}

enum ActorMessage {
    Shutdown,
}

fn run(receiver: Receiver<ActorMessage>) {
    loop {
        if let Ok(actor_message) = receiver.try_recv() {
            match actor_message {
                ActorMessage::Shutdown => {
                    break;
                }
            }
        }
    }
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
            thread::sleep(Duration::from_millis(500));

            clone.shutdown();
        });

        dht.block_until_shutdown();
    }
}
