//! Modules needed only for nodes running in server mode (not read-only).

pub mod peers;
pub mod tokens;

use std::{net::SocketAddr, num::NonZeroUsize};

use bytes::Bytes;
use lru::LruCache;
use tracing::debug;

use crate::{
    common::{
        validate_immutable, AnnouncePeerRequestArguments, ErrorSpecific, FindNodeRequestArguments,
        FindNodeResponseArguments, GetImmutableResponseArguments, GetMutableResponseArguments,
        GetPeersRequestArguments, GetPeersResponseArguments, GetValueRequestArguments, Id,
        MutableItem, NoMoreRecentValueResponseArguments, NoValuesResponseArguments,
        PingResponseArguments, PutImmutableRequestArguments, PutMutableRequestArguments,
        PutRequest, PutRequestSpecific, RequestSpecific, RequestTypeSpecific, ResponseSpecific,
    },
    rpc::Rpc,
};

use peers::PeersStore;
use tokens::Tokens;

// Stored data in server mode.
pub const MAX_INFO_HASHES: usize = 2000;
pub const MAX_PEERS: usize = 500;
pub const MAX_VALUES: usize = 1000;

/// Dht server that can handle incoming rpc requests
pub trait Server: std::fmt::Debug + Send + Sync {
    /// Handle incoming requests.
    ///
    /// This function will block the main loop where the [Rpc]
    /// is running, thus it needs to be very fast and lightweight.
    fn handle_request(
        &mut self,
        rpc: &mut Rpc,
        from: SocketAddr,
        transaction_id: u16,
        request: &RequestSpecific,
    );
}

#[derive(Debug)]
/// Default implementation of [Server] trait.
///
/// Supports [BEP0005](https://www.bittorrent.org/beps/bep_0005.html) and [BEP_0044](https://www.bittorrent.org/beps/bep_0044.html).
///
/// But it doesn't implement any rate-limiting or blocking.
pub struct DefaultServer {
    tokens: Tokens,
    // server storage
    peers: PeersStore,

    immutable_values: LruCache<Id, Bytes>,
    mutable_values: LruCache<Id, MutableItem>,
}

impl Default for DefaultServer {
    fn default() -> Self {
        DefaultServer::new(&DefaultServerSettings::default())
    }
}

#[derive(Debug, Clone, Default)]
pub struct DefaultServerSettings {
    /// The maximum info_hashes for which to store peers.
    ///
    /// Defaults to [MAX_INFO_HASHES]
    pub max_info_hashes: usize,
    /// The maximum peers to store per info_hash.
    ///
    /// Defaults to [MAX_PEERS]
    pub max_peers_per_info_hash: usize,
    /// Maximum number of immutable values to store.
    ///
    /// Defaults to [MAX_VALUES]
    pub max_immutable_values: usize,
    /// Maximum number of mutable values to store.
    ///
    /// Defaults to [MAX_VALUES]
    pub max_mutable_values: usize,
}

impl DefaultServer {
    pub fn new(settings: &DefaultServerSettings) -> Self {
        let tokens = Tokens::new();

        Self {
            tokens,
            peers: PeersStore::new(
                NonZeroUsize::new(settings.max_info_hashes).unwrap_or(
                    NonZeroUsize::new(MAX_INFO_HASHES).expect("MAX_PEERS is NonZeroUsize"),
                ),
                NonZeroUsize::new(settings.max_peers_per_info_hash)
                    .unwrap_or(NonZeroUsize::new(MAX_PEERS).expect("MAX_PEERS is NonZeroUsize")),
            ),

            immutable_values: LruCache::new(
                NonZeroUsize::new(settings.max_immutable_values)
                    .unwrap_or(NonZeroUsize::new(MAX_VALUES).expect("MAX_VALUES is NonZeroUsize")),
            ),
            mutable_values: LruCache::new(
                NonZeroUsize::new(settings.max_mutable_values)
                    .unwrap_or(NonZeroUsize::new(MAX_VALUES).expect("MAX_VALUES is NonZeroUsize")),
            ),
        }
    }

    // === Private Methods ===

    /// Handle get mutable request
    fn handle_get_mutable(
        &mut self,
        rpc: &mut Rpc,
        from: SocketAddr,
        transaction_id: u16,
        target: &Id,
        seq: &Option<i64>,
    ) {
        rpc.response(
            from,
            transaction_id,
            match self.mutable_values.get(target) {
                Some(item) => {
                    let no_more_recent_values = seq.map(|request_seq| item.seq() <= &request_seq);

                    match no_more_recent_values {
                        Some(true) => ResponseSpecific::NoMoreRecentValue(
                            NoMoreRecentValueResponseArguments {
                                responder_id: *rpc.id(),
                                token: self.tokens.generate_token(from).into(),
                                nodes: Some(rpc.routing_table().closest(target)),
                                seq: *item.seq(),
                            },
                        ),
                        _ => ResponseSpecific::GetMutable(GetMutableResponseArguments {
                            responder_id: *rpc.id(),
                            token: self.tokens.generate_token(from).into(),
                            nodes: Some(rpc.routing_table().closest(target)),
                            v: item.value().to_vec(),
                            k: item.key().to_vec(),
                            seq: *item.seq(),
                            sig: item.signature().to_vec(),
                        }),
                    }
                }
                None => ResponseSpecific::NoValues(NoValuesResponseArguments {
                    responder_id: *rpc.id(),
                    token: self.tokens.generate_token(from).into(),
                    nodes: Some(rpc.routing_table().closest(target)),
                }),
            },
        )
    }
}

impl Server for DefaultServer {
    /// Handle incoming request.
    fn handle_request(
        &mut self,
        rpc: &mut Rpc,
        from: SocketAddr,
        transaction_id: u16,
        request: &RequestSpecific,
    ) {
        // Lazily rotate secrets before handling a request
        if self.tokens.should_update() {
            self.tokens.rotate()
        }

        let requester_id = request.requester_id;

        match &request.request_type {
            RequestTypeSpecific::Ping => {
                rpc.response(
                    from,
                    transaction_id,
                    ResponseSpecific::Ping(PingResponseArguments {
                        responder_id: *rpc.id(),
                    }),
                );
            }
            RequestTypeSpecific::FindNode(FindNodeRequestArguments { target, .. }) => {
                rpc.response(
                    from,
                    transaction_id,
                    ResponseSpecific::FindNode(FindNodeResponseArguments {
                        responder_id: *rpc.id(),
                        nodes: rpc.routing_table().closest(target),
                    }),
                );
            }
            RequestTypeSpecific::GetPeers(GetPeersRequestArguments { info_hash, .. }) => {
                rpc.response(
                    from,
                    transaction_id,
                    match self.peers.get_random_peers(info_hash) {
                        Some(peers) => ResponseSpecific::GetPeers(GetPeersResponseArguments {
                            responder_id: *rpc.id(),
                            token: self.tokens.generate_token(from).into(),
                            nodes: Some(rpc.routing_table().closest(info_hash)),
                            values: peers,
                        }),
                        None => ResponseSpecific::NoValues(NoValuesResponseArguments {
                            responder_id: *rpc.id(),
                            token: self.tokens.generate_token(from).into(),
                            nodes: Some(rpc.routing_table().closest(info_hash)),
                        }),
                    },
                );
            }
            RequestTypeSpecific::GetValue(GetValueRequestArguments { target, seq, .. }) => {
                if seq.is_some() {
                    return self.handle_get_mutable(rpc, from, transaction_id, target, seq);
                }

                if let Some(v) = self.immutable_values.get(target) {
                    rpc.response(
                        from,
                        transaction_id,
                        ResponseSpecific::GetImmutable(GetImmutableResponseArguments {
                            responder_id: *rpc.id(),
                            token: self.tokens.generate_token(from).into(),
                            nodes: Some(rpc.routing_table().closest(target)),
                            v: v.to_vec(),
                        }),
                    )
                } else {
                    self.handle_get_mutable(rpc, from, transaction_id, target, seq);
                };
            }
            RequestTypeSpecific::Put(PutRequest {
                token,
                put_request_type,
            }) => match put_request_type {
                PutRequestSpecific::AnnouncePeer(AnnouncePeerRequestArguments {
                    info_hash,
                    port,
                    implied_port,
                    ..
                }) => {
                    if !self.tokens.validate(from, token) {
                        rpc.error(
                            from,
                            transaction_id,
                            ErrorSpecific {
                                code: 203,
                                description: "Bad token".to_string(),
                            },
                        );
                        debug!(
                            ?info_hash,
                            ?requester_id,
                            ?from,
                            ?token,
                            request_type = "announce_peer",
                            "Invalid token"
                        );
                        return;
                    }

                    let peer = match implied_port {
                        Some(true) => from,
                        _ => SocketAddr::new(from.ip(), *port),
                    };

                    self.peers
                        .add_peer(*info_hash, (&request.requester_id, peer));

                    rpc.response(
                        from,
                        transaction_id,
                        ResponseSpecific::Ping(PingResponseArguments {
                            responder_id: *rpc.id(),
                        }),
                    );
                }
                PutRequestSpecific::PutImmutable(PutImmutableRequestArguments {
                    v,
                    target,
                    ..
                }) => {
                    if !self.tokens.validate(from, token) {
                        rpc.error(
                            from,
                            transaction_id,
                            ErrorSpecific {
                                code: 203,
                                description: "Bad token".to_string(),
                            },
                        );
                        debug!(
                            ?target,
                            ?requester_id,
                            ?from,
                            ?token,
                            request_type = "put_immutable",
                            "Invalid token"
                        );
                        return;
                    }

                    if v.len() > 1000 {
                        rpc.error(
                            from,
                            transaction_id,
                            ErrorSpecific {
                                code: 205,
                                description: "Message (v field) too big.".to_string(),
                            },
                        );
                        debug!(?target, ?requester_id, ?from, size = ?v.len(), "Message (v field) too big.");
                        return;
                    }
                    if !validate_immutable(v, target) {
                        rpc.error(
                            from,
                            transaction_id,
                            ErrorSpecific {
                                code: 203,
                                description: "Target doesn't match the sha1 hash of v field"
                                    .to_string(),
                            },
                        );
                        debug!(?target, ?requester_id, ?from, v = ?v, "Target doesn't match the sha1 hash of v field.");
                        return;
                    }

                    self.immutable_values.put(*target, v.to_owned().into());

                    rpc.response(
                        from,
                        transaction_id,
                        ResponseSpecific::Ping(PingResponseArguments {
                            responder_id: *rpc.id(),
                        }),
                    );
                }
                PutRequestSpecific::PutMutable(PutMutableRequestArguments {
                    target,
                    v,
                    k,
                    seq,
                    sig,
                    salt,
                    cas,
                    ..
                }) => {
                    if !self.tokens.validate(from, token) {
                        rpc.error(
                            from,
                            transaction_id,
                            ErrorSpecific {
                                code: 203,
                                description: "Bad token".to_string(),
                            },
                        );
                        debug!(
                            ?target,
                            ?requester_id,
                            ?from,
                            ?token,
                            request_type = "put_mutable",
                            "Invalid token"
                        );
                        return;
                    }
                    if v.len() > 1000 {
                        rpc.error(
                            from,
                            transaction_id,
                            ErrorSpecific {
                                code: 205,
                                description: "Message (v field) too big.".to_string(),
                            },
                        );
                        return;
                    }
                    if let Some(salt) = salt {
                        if salt.len() > 64 {
                            rpc.error(
                                from,
                                transaction_id,
                                ErrorSpecific {
                                    code: 207,
                                    description: "salt (salt field) too big.".to_string(),
                                },
                            );
                            return;
                        }
                    }
                    if let Some(previous) = self.mutable_values.get(target) {
                        if let Some(cas) = cas {
                            if previous.seq() != cas {
                                rpc.error(
                                    from,
                                    transaction_id,
                                    ErrorSpecific {
                                        code: 301,
                                        description: "CAS mismatched, re-read value and try again."
                                            .to_string(),
                                    },
                                );
                                debug!(
                                    ?target,
                                    ?requester_id,
                                    ?from,
                                    "CAS mismatched, re-read value and try again."
                                );

                                return;
                            }
                        };

                        if seq <= previous.seq() {
                            rpc.error(
                                from,
                                transaction_id,
                                ErrorSpecific {
                                    code: 302,
                                    description: "Sequence number less than current.".to_string(),
                                },
                            );
                            debug!(
                                ?target,
                                ?requester_id,
                                ?from,
                                "Sequence number less than current."
                            );

                            return;
                        }
                    }

                    match MutableItem::from_dht_message(
                        target,
                        k,
                        v.to_owned().into(),
                        seq,
                        sig,
                        salt.to_owned().map(|v| v.into()),
                        cas,
                    ) {
                        Ok(item) => {
                            self.mutable_values.put(*target, item);

                            rpc.response(
                                from,
                                transaction_id,
                                ResponseSpecific::Ping(PingResponseArguments {
                                    responder_id: *rpc.id(),
                                }),
                            );
                        }
                        Err(error) => {
                            rpc.error(
                                from,
                                transaction_id,
                                ErrorSpecific {
                                    code: 206,
                                    description: "Invalid signature".to_string(),
                                },
                            );

                            debug!(?target, ?requester_id, ?from, ?error, "Invalid signature");
                        }
                    }
                }
            },
        }
    }
}
