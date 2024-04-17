//! Request hanlders

use std::net::SocketAddr;

use tracing::debug;

use crate::common::{
    messages::{
        AnnouncePeerRequestArguments, ErrorSpecific, FindNodeRequestArguments,
        FindNodeResponseArguments, GetImmutableResponseArguments, GetMutableResponseArguments,
        GetPeersRequestArguments, GetPeersResponseArguments, GetValueRequestArguments,
        NoMoreRecentValueResponseArguments, NoValuesResponseArguments, PingResponseArguments,
        PutImmutableRequestArguments, PutMutableRequestArguments, PutRequest, PutRequestSpecific,
        RequestSpecific, RequestTypeSpecific, ResponseSpecific,
    },
    validate_immutable, Id, MutableItem,
};

use super::super::Rpc;

pub fn handle_request(
    rpc: &mut Rpc,
    from: SocketAddr,
    transaction_id: u16,
    request: &RequestSpecific,
) {
    let requester_id = request.requester_id;

    match &request.request_type {
        RequestTypeSpecific::Ping => {
            rpc.socket.response(
                from,
                transaction_id,
                ResponseSpecific::Ping(PingResponseArguments {
                    responder_id: rpc.id,
                }),
            );
        }
        RequestTypeSpecific::FindNode(FindNodeRequestArguments { target, .. }) => {
            rpc.socket.response(
                from,
                transaction_id,
                ResponseSpecific::FindNode(FindNodeResponseArguments {
                    responder_id: rpc.id,
                    nodes: rpc.routing_table.closest(target),
                }),
            );
        }
        RequestTypeSpecific::GetPeers(GetPeersRequestArguments { info_hash, .. }) => {
            rpc.socket.response(
                from,
                transaction_id,
                match rpc.peers.get_random_peers(info_hash) {
                    Some(peers) => ResponseSpecific::GetPeers(GetPeersResponseArguments {
                        responder_id: rpc.id,
                        token: rpc.tokens.generate_token(from).into(),
                        nodes: Some(rpc.routing_table.closest(info_hash)),
                        values: peers,
                    }),
                    None => ResponseSpecific::NoValues(NoValuesResponseArguments {
                        responder_id: rpc.id,
                        token: rpc.tokens.generate_token(from).into(),
                        nodes: Some(rpc.routing_table.closest(info_hash)),
                    }),
                },
            );
        }
        RequestTypeSpecific::GetValue(GetValueRequestArguments { target, seq, .. }) => {
            if seq.is_some() {
                return handle_get_mutable(rpc, from, transaction_id, target, seq);
            }

            if let Some(v) = rpc.immutable_values.get(target) {
                rpc.socket.response(
                    from,
                    transaction_id,
                    ResponseSpecific::GetImmutable(GetImmutableResponseArguments {
                        responder_id: rpc.id,
                        token: rpc.tokens.generate_token(from).into(),
                        nodes: Some(rpc.routing_table.closest(target)),
                        v: v.to_vec(),
                    }),
                )
            } else {
                handle_get_mutable(rpc, from, transaction_id, target, seq);
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
                if !rpc.tokens.validate(from, token) {
                    rpc.socket.error(
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

                rpc.peers
                    .add_peer(*info_hash, (&request.requester_id, peer));

                rpc.socket.response(
                    from,
                    transaction_id,
                    ResponseSpecific::Ping(PingResponseArguments {
                        responder_id: rpc.id,
                    }),
                );
            }
            PutRequestSpecific::PutImmutable(PutImmutableRequestArguments {
                v, target, ..
            }) => {
                if !rpc.tokens.validate(from, token) {
                    rpc.socket.error(
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
                    rpc.socket.error(
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
                    rpc.socket.error(
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

                rpc.immutable_values.put(*target, v.to_owned().into());

                rpc.socket.response(
                    from,
                    transaction_id,
                    ResponseSpecific::Ping(PingResponseArguments {
                        responder_id: rpc.id,
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
                if !rpc.tokens.validate(from, token) {
                    rpc.socket.error(
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
                    rpc.socket.error(
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
                        rpc.socket.error(
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
                if let Some(previous) = rpc.mutable_values.get(target) {
                    if let Some(cas) = cas {
                        if previous.seq() != cas {
                            rpc.socket.error(
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
                        rpc.socket.error(
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
                        rpc.mutable_values.put(*target, item);

                        rpc.socket.response(
                            from,
                            transaction_id,
                            ResponseSpecific::Ping(PingResponseArguments {
                                responder_id: rpc.id,
                            }),
                        );
                    }
                    Err(error) => {
                        rpc.socket.error(
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

fn handle_get_mutable(
    rpc: &mut Rpc,
    from: SocketAddr,
    transaction_id: u16,
    target: &Id,
    seq: &Option<i64>,
) {
    rpc.socket.response(
        from,
        transaction_id,
        match rpc.mutable_values.get(target) {
            Some(item) => {
                let no_more_recent_values = seq.map(|request_seq| item.seq() <= &request_seq);

                match no_more_recent_values {
                    Some(true) => {
                        ResponseSpecific::NoMoreRecentValue(NoMoreRecentValueResponseArguments {
                            responder_id: rpc.id,
                            token: rpc.tokens.generate_token(from).into(),
                            nodes: Some(rpc.routing_table.closest(target)),
                            seq: *item.seq(),
                        })
                    }
                    _ => ResponseSpecific::GetMutable(GetMutableResponseArguments {
                        responder_id: rpc.id,
                        token: rpc.tokens.generate_token(from).into(),
                        nodes: Some(rpc.routing_table.closest(target)),
                        v: item.value().to_vec(),
                        k: item.key().to_vec(),
                        seq: *item.seq(),
                        sig: item.signature().to_vec(),
                    }),
                }
            }
            None => ResponseSpecific::NoValues(NoValuesResponseArguments {
                responder_id: rpc.id,
                token: rpc.tokens.generate_token(from).into(),
                nodes: Some(rpc.routing_table.closest(target)),
            }),
        },
    )
}
