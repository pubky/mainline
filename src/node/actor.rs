use std::collections::HashMap;
use std::net::SocketAddrV4;

use flume::{Receiver, RecvError, Sender, TryRecvError};
use tracing::info;

use crate::rpc::config::Config;
use crate::rpc::{GetRequestSpecific, Info, PutError, Response, Rpc};
use crate::{Id, MutableItem, Node, PutRequestSpecific};

#[derive(Debug)]
pub struct Actor {
    rpc: Rpc,
    receiver: Receiver<ActorMessage>,
    get_senders: HashMap<Id, Vec<ResponseSender>>,
    put_senders: HashMap<Id, Vec<Sender<Result<Id, PutError>>>>,
}

impl Actor {
    pub fn new(config: Config, receiver: Receiver<ActorMessage>) -> std::io::Result<Self> {
        let rpc = Rpc::new(config)?;

        let address = rpc.local_addr();
        info!(?address, "Mainline DHT listening");

        Ok(Self {
            rpc,
            receiver,
            get_senders: HashMap::new(),
            put_senders: HashMap::new(),
        })
    }

    /// Returns an error if the actor's sender is dropped.
    pub fn tick(&mut self) -> Result<(), RecvError> {
        match self.receiver.try_recv() {
            Ok(actor_message) => match actor_message {
                ActorMessage::Check(sender) => {
                    let _ = sender.send(Ok(()));
                }
                ActorMessage::Info(sender) => {
                    let _ = sender.send(self.rpc.info());
                }
                ActorMessage::Put(request, sender, extra_nodes) => {
                    let target = *request.target();

                    match self.rpc.put(request, extra_nodes) {
                        Ok(()) => {
                            let senders = self.put_senders.entry(target).or_default();

                            senders.push(sender);
                        }
                        Err(error) => {
                            let _ = sender.send(Err(error));
                        }
                    };
                }
                ActorMessage::Get(request, sender) => {
                    let target = *request.target();

                    if let Some(responses) = self.rpc.get(request, None) {
                        for response in responses {
                            send(&sender, response);
                        }
                    };

                    let senders = self.get_senders.entry(target).or_default();

                    senders.push(sender);
                }
                ActorMessage::ToBootstrap(sender) => {
                    let _ = sender.send(self.rpc.routing_table().to_bootstrap());
                }
            },
            Err(TryRecvError::Disconnected) => {
                // Node was dropped, kill this thread.
                tracing::debug!("mainline::Dht's actor thread was shutdown after Drop.");
                return Err(RecvError::Disconnected);
            }
            Err(TryRecvError::Empty) => {
                // No op
            }
        }

        let report = self.rpc.tick();

        // Response for an ongoing GET query
        if let Some((target, response)) = report.new_query_response {
            if let Some(senders) = self.get_senders.get(&target) {
                for sender in senders {
                    send(sender, response.clone());
                }
            }
        }

        // Cleanup done GET queries
        for (id, closest_nodes) in report.done_get_queries {
            if let Some(senders) = self.get_senders.remove(&id) {
                for sender in senders {
                    // return closest_nodes to whoever was asking
                    if let ResponseSender::ClosestNodes(sender) = sender {
                        let _ = sender.send(closest_nodes.clone());
                    }
                }
            }
        }

        // Cleanup done PUT query and send a resulting error if any.
        for (id, error) in report.done_put_queries {
            if let Some(senders) = self.put_senders.remove(&id) {
                let result = if let Some(error) = error {
                    Err(error)
                } else {
                    Ok(id)
                };

                for sender in senders {
                    let _ = sender.send(result.clone());
                }
            }
        }

        Ok(())
    }
}

fn send(sender: &ResponseSender, response: Response) {
    match (sender, response) {
        (ResponseSender::Peers(s), Response::Peers(r)) => {
            let _ = s.send(r);
        }
        (ResponseSender::Mutable(s), Response::Mutable(r)) => {
            let _ = s.send(r);
        }
        (ResponseSender::Immutable(s), Response::Immutable(r)) => {
            let _ = s.send(r);
        }
        _ => {}
    }
}

#[derive(Debug)]
pub(crate) enum ActorMessage {
    Info(Sender<Info>),
    Put(
        PutRequestSpecific,
        Sender<Result<Id, PutError>>,
        Option<Box<[Node]>>,
    ),
    Get(GetRequestSpecific, ResponseSender),
    Check(Sender<Result<(), std::io::Error>>),
    ToBootstrap(Sender<Vec<String>>),
}

#[derive(Debug, Clone)]
pub enum ResponseSender {
    ClosestNodes(Sender<Box<[Node]>>),
    Peers(Sender<Vec<SocketAddrV4>>),
    Mutable(Sender<MutableItem>),
    Immutable(Sender<Box<[u8]>>),
}
