use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddrV4},
};

use serde_bencode::value::Value;

use super::{compact_nodes::NodeContact, query::*, *};

mod bep44;

const ID: Id = ByteArray(*b"abcdefghij0123456789");
const TARGET: Id = ByteArray(*b"0123456789abcdefghij");
const PING: &[u8] = b"d1:ad2:id20:abcdefghij0123456789e1:q4:ping1:t2:aa1:y1:qe";

fn query(query: WireQuery) -> WireMessage {
    WireMessage {
        transaction_id: ByteString(b"aa".to_vec()),
        version: None,
        kind: WireKind::Query {
            query,
            read_only: OptionalBool(false),
        },
    }
}

fn ping() -> WireMessage {
    query(WireQuery::Ping {
        arguments: Map(PingArguments { id: ID }),
    })
}

fn response_arguments() -> ResponseArguments {
    ResponseArguments {
        id: ID,
        token: None,
        nodes: None,
        values: None,
        value: None,
        key: None,
        signature: None,
        seq: None,
    }
}

fn response(arguments: ResponseArguments) -> WireMessage {
    WireMessage {
        transaction_id: ByteString(b"aa".to_vec()),
        version: None,
        kind: WireKind::Response {
            arguments: Map(arguments),
            requester_address: None,
        },
    }
}

fn assert_wire(message: &WireMessage, bytes: &[u8]) {
    assert_eq!(serde_bencode::to_bytes(message).unwrap(), bytes);
    assert_eq!(
        serde_bencode::from_bytes::<WireMessage>(bytes).unwrap(),
        *message
    );
}

fn assert_roundtrip(message: &WireMessage) -> Value {
    let bytes = serde_bencode::to_bytes(message).unwrap();
    assert_eq!(
        serde_bencode::from_bytes::<WireMessage>(&bytes).unwrap(),
        *message
    );
    serde_bencode::from_bytes(&bytes).unwrap()
}

fn dictionary(value: &mut Value) -> &mut HashMap<Vec<u8>, Value> {
    let Value::Dict(dictionary) = value else {
        panic!("expected dictionary");
    };
    dictionary
}

fn argument_dictionary<'a>(value: &'a mut Value, key: &[u8]) -> &'a mut HashMap<Vec<u8>, Value> {
    let arguments = dictionary(value)
        .get_mut(key)
        .expect("expected argument field");
    dictionary(arguments)
}

fn decode(value: &Value) -> Result<WireMessage, serde_bencode::Error> {
    serde_bencode::from_bytes(&serde_bencode::to_bytes(value).unwrap())
}

#[test]
fn ping_matches_wire_example() {
    assert_wire(&ping(), PING);
}

#[test]
fn transaction_id_and_version_preserve_binary_strings() {
    for transaction_id in [vec![], vec![0, 255, 128]] {
        let mut message = ping();
        message.transaction_id = ByteString(transaction_id);
        message.version = Some(ByteString(vec![255, 0]));
        assert_roundtrip(&message);
    }
}

#[test]
fn envelope_requires_transaction_id_and_kind() {
    for field in [b"t", b"y"] {
        let mut value = assert_roundtrip(&ping());
        dictionary(&mut value).remove(field.as_slice());
        assert!(decode(&value).is_err());
    }
    for bytes in [b"le".as_slice(), b"d1:t2:aa1:y1:xe", b"d1:ti1e1:y1:qe"] {
        assert!(serde_bencode::from_bytes::<WireMessage>(bytes).is_err());
    }
}

#[test]
fn query_ignores_unknown_and_inapplicable_fields() {
    let mut value = assert_roundtrip(&ping());
    for field in [b"lol".as_slice(), b"r", b"e", b"ip"] {
        dictionary(&mut value).insert(field.to_vec(), Value::Bytes(b"haha".to_vec()));
    }
    assert_eq!(decode(&value).unwrap(), ping());
}

#[test]
fn ping_and_announce_peer_response_matches_wire_example() {
    let message = response(ResponseArguments {
        id: ByteArray(*b"mnopqrstuvwxyz123456"),
        ..response_arguments()
    });
    let bytes = b"d1:rd2:id20:mnopqrstuvwxyz123456e1:t2:aa1:y1:re";
    assert_wire(&message, bytes);
}

#[test]
fn response_parses_compact_nodes() {
    let contact = NodeContact {
        id: TARGET,
        address: SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 6881),
    };
    for token in [None, Some(ByteString(vec![0, 255]))] {
        let message = response(ResponseArguments {
            nodes: Some(CompactNodes(vec![contact])),
            token,
            ..response_arguments()
        });
        let mut value = assert_roundtrip(&message);
        let arguments = argument_dictionary(&mut value, b"r");
        assert_eq!(
            arguments.get(b"nodes".as_slice()),
            Some(&Value::Bytes(
                b"0123456789abcdefghij\x01\x02\x03\x04\x1a\xe1".to_vec()
            ))
        );
    }
}

#[test]
fn get_peers_response_matches_wire_example() {
    let bytes = b"d1:rd2:id20:abcdefghij01234567895:token8:aoeusnth6:valuesl6:axje.u6:idhtnmee1:t2:aa1:y1:re";
    let expected = response(ResponseArguments {
        token: Some(ByteString(b"aoeusnth".to_vec())),
        values: Some(vec![
            CompactAddress(SocketAddrV4::new(Ipv4Addr::new(97, 120, 106, 101), 11893)),
            CompactAddress(SocketAddrV4::new(Ipv4Addr::new(105, 100, 104, 116), 28269)),
        ]),
        ..response_arguments()
    });
    assert_wire(&expected, bytes);
}

#[test]
fn response_preserves_absent_and_empty_fields() {
    for present in [false, true] {
        let mut value = assert_roundtrip(&response(ResponseArguments {
            token: present.then(|| ByteString(vec![])),
            nodes: present.then(|| CompactNodes(vec![])),
            values: present.then(Vec::new),
            ..response_arguments()
        }));
        let arguments = argument_dictionary(&mut value, b"r");
        for field in [b"token".as_slice(), b"nodes", b"values"] {
            assert_eq!(arguments.contains_key(field), present);
        }
    }
}

#[test]
fn response_preserves_combined_nodes_and_peers() {
    let contact = NodeContact {
        id: TARGET,
        address: SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 6881),
    };
    assert_roundtrip(&response(ResponseArguments {
        token: Some(ByteString(vec![])),
        nodes: Some(CompactNodes(vec![contact])),
        values: Some(vec![CompactAddress(contact.address)]),
        ..response_arguments()
    }));
}

#[test]
fn response_requires_twenty_byte_id() {
    let mut value = assert_roundtrip(&response(response_arguments()));
    let arguments = argument_dictionary(&mut value, b"r");
    arguments.remove(b"id".as_slice());
    assert!(decode(&value).is_err());
    for id in [
        Value::Bytes(vec![0; 19]),
        Value::Bytes(vec![0; 21]),
        Value::List(vec![Value::Int(0); 20]),
    ] {
        let arguments = argument_dictionary(&mut value, b"r");
        arguments.insert(b"id".to_vec(), id);
        assert!(decode(&value).is_err());
    }
}

#[test]
fn response_rejects_malformed_known_fields() {
    for (field, invalid) in [
        ("token", Value::Int(1)),
        ("token", Value::List(vec![Value::Int(1)])),
        ("nodes", Value::Bytes(vec![0; 25])),
        ("nodes", Value::List(vec![])),
        ("values", Value::Bytes(vec![0; 6])),
        ("values", Value::List(vec![Value::Bytes(vec![0; 5])])),
        ("values", Value::List(vec![Value::Bytes(vec![0; 7])])),
        (
            "values",
            Value::List(vec![Value::List(vec![Value::Int(0); 6])]),
        ),
    ] {
        let mut value = assert_roundtrip(&response(response_arguments()));
        let arguments = argument_dictionary(&mut value, b"r");
        arguments.insert(field.as_bytes().to_vec(), invalid);
        assert!(decode(&value).is_err(), "invalid {field}");
    }
}

#[test]
fn response_ignores_unknown_fields() {
    let message = response(response_arguments());
    let mut value = assert_roundtrip(&message);
    let arguments = argument_dictionary(&mut value, b"r");
    for key in [b"unknown".to_vec(), b"nodes6".to_vec(), vec![255]] {
        arguments.insert(key, Value::Int(42));
    }
    assert_eq!(decode(&value).unwrap(), message);
}

#[test]
fn response_requires_dictionary_arguments() {
    for bytes in [b"d1:t2:aa1:y1:re".as_slice(), b"d1:rle1:t2:aa1:y1:re"] {
        assert!(serde_bencode::from_bytes::<WireMessage>(bytes).is_err());
    }
    let mut value = assert_roundtrip(&response(response_arguments()));
    dictionary(&mut value).insert(
        b"r".to_vec(),
        Value::List(vec![
            Value::Bytes(ID.0.to_vec()),
            Value::Bytes(vec![]),
            Value::Bytes(vec![]),
            Value::List(vec![]),
            Value::Int(1),
            Value::Bytes(vec![0; 32]),
            Value::Bytes(vec![0; 64]),
            Value::Int(1),
        ]),
    );
    assert!(decode(&value).is_err());
}

#[test]
fn error_matches_wire_example() {
    let message = WireMessage {
        transaction_id: ByteString(b"aa".to_vec()),
        version: None,
        kind: WireKind::Error {
            error: (
                ErrorCode::GENERIC,
                ByteString(b"A Generic Error Ocurred".to_vec()),
            ),
            requester_address: None,
        },
    };
    assert_wire(
        &message,
        b"d1:eli201e23:A Generic Error Ocurrede1:t2:aa1:y1:ee",
    );
}

#[test]
fn error_requires_code_and_description_pair() {
    for bytes in [
        b"d1:t2:aa1:y1:ee".as_slice(),
        b"d1:ele1:t2:aa1:y1:ee",
        b"d1:eli201ee1:t2:aa1:y1:ee",
        b"d1:eli201e3:bad0:e1:t2:aa1:y1:ee",
        b"d1:el3:badi201ee1:t2:aa1:y1:ee",
    ] {
        assert!(serde_bencode::from_bytes::<WireMessage>(bytes).is_err());
    }
}

#[test]
fn read_only_accepts_zero_and_one_and_omits_false() {
    let mut value = assert_roundtrip(&ping());
    assert!(!dictionary(&mut value).contains_key(b"ro".as_slice()));
    for (encoded, expected) in [(0, false), (1, true)] {
        dictionary(&mut value).insert(b"ro".to_vec(), Value::Int(encoded));
        let message = decode(&value).unwrap();
        let WireKind::Query { read_only, .. } = &message.kind else {
            panic!("expected query");
        };
        assert_eq!(read_only.0, expected);
        let mut serialized = assert_roundtrip(&message);
        assert_eq!(
            dictionary(&mut serialized).get(b"ro".as_slice()),
            expected.then_some(&Value::Int(1))
        );
    }
}

#[test]
fn response_and_error_preserve_requester_address_and_ignore_read_only() {
    let address = CompactAddress(SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 6881));
    for kind in [
        WireKind::Response {
            arguments: Map(response_arguments()),
            requester_address: Some(address),
        },
        WireKind::Error {
            error: (ErrorCode::GENERIC, ByteString(vec![0, 255])),
            requester_address: Some(address),
        },
    ] {
        let message = WireMessage { kind, ..ping() };
        let mut value = assert_roundtrip(&message);
        assert!(!dictionary(&mut value).contains_key(b"ro".as_slice()));
        dictionary(&mut value).insert(b"ro".to_vec(), Value::Bytes(b"ignored".to_vec()));
        assert_eq!(decode(&value).unwrap(), message);
        dictionary(&mut value).insert(b"ip".to_vec(), Value::Bytes(vec![0; 5]));
        assert!(decode(&value).is_err());
    }
}

#[test]
fn query_requires_known_method_and_dictionary_arguments() {
    for field in [b"q", b"a"] {
        let mut value = assert_roundtrip(&ping());
        dictionary(&mut value).remove(field.as_slice());
        assert!(decode(&value).is_err());
    }
    let mut value = assert_roundtrip(&ping());
    dictionary(&mut value).insert(b"q".to_vec(), Value::Bytes(b"unknown".to_vec()));
    assert!(decode(&value).is_err());
    let mut value = assert_roundtrip(&ping());
    dictionary(&mut value).insert(
        b"a".to_vec(),
        Value::List(vec![Value::Bytes(ID.0.to_vec())]),
    );
    assert!(decode(&value).is_err());
}

#[test]
fn query_commands_roundtrip_and_require_arguments() {
    let cases = [
        (
            WireQuery::Ping {
                arguments: Map(PingArguments { id: ID }),
            },
            "ping",
            vec!["id"],
        ),
        (
            WireQuery::FindNode {
                arguments: Map(FindNodeArguments {
                    id: ID,
                    target: TARGET,
                }),
            },
            "find_node",
            vec!["id", "target"],
        ),
        (
            WireQuery::GetPeers {
                arguments: Map(GetPeersArguments {
                    id: ID,
                    info_hash: TARGET,
                }),
            },
            "get_peers",
            vec!["id", "info_hash"],
        ),
        (
            WireQuery::AnnouncePeer {
                arguments: Map(AnnouncePeerArguments {
                    id: ID,
                    info_hash: TARGET,
                    port: 6881,
                    implied_port: OptionalBool(true),
                    token: ByteString(vec![0, 255]),
                }),
            },
            "announce_peer",
            vec!["id", "info_hash", "port", "token"],
        ),
        (
            WireQuery::Get {
                arguments: Map(GetArguments {
                    id: ID,
                    target: TARGET,
                    seq: Some(42),
                }),
            },
            "get",
            vec!["id", "target"],
        ),
        (
            WireQuery::Put {
                arguments: Map(PutArguments {
                    id: ID,
                    token: ByteString(vec![]),
                    value: Value::Bytes(b"item".to_vec()),
                    key: None,
                    signature: None,
                    seq: None,
                    salt: None,
                    cas: None,
                }),
            },
            "put",
            vec!["id", "token", "v"],
        ),
    ];
    for (command, method, required_fields) in cases {
        let mut value = assert_roundtrip(&query(command));
        assert_eq!(
            dictionary(&mut value).get(b"q".as_slice()),
            Some(&Value::Bytes(method.as_bytes().to_vec()))
        );
        for field in required_fields {
            let mut missing = value.clone();
            let arguments = argument_dictionary(&mut missing, b"a");
            arguments.remove(field.as_bytes());
            assert!(decode(&missing).is_err(), "{method} requires {field}");
        }
        let arguments = argument_dictionary(&mut value, b"a");
        arguments.insert(b"id".to_vec(), Value::Bytes(vec![0; 19]));
        assert!(decode(&value).is_err(), "{method} requires a 20-byte ID");
    }
}

#[test]
fn announce_peer_defaults_implied_port_to_false() {
    let mut value = assert_roundtrip(&query(WireQuery::AnnouncePeer {
        arguments: Map(AnnouncePeerArguments {
            id: ID,
            info_hash: TARGET,
            port: 6881,
            implied_port: OptionalBool(false),
            token: ByteString(vec![]),
        }),
    }));
    let arguments = argument_dictionary(&mut value, b"a");
    arguments.remove(b"implied_port".as_slice());
    let WireKind::Query {
        query: WireQuery::AnnouncePeer { arguments },
        ..
    } = decode(&value).unwrap().kind
    else {
        panic!("expected announce_peer");
    };
    assert!(!arguments.0.implied_port.0);
}

#[test]
fn announce_peer_rejects_out_of_range_ports() {
    let mut value = assert_roundtrip(&query(WireQuery::AnnouncePeer {
        arguments: Map(AnnouncePeerArguments {
            id: ID,
            info_hash: TARGET,
            port: 6881,
            implied_port: OptionalBool(false),
            token: ByteString(vec![]),
        }),
    }));
    for port in [-1, 65536] {
        let arguments = argument_dictionary(&mut value, b"a");
        arguments.insert(b"port".to_vec(), Value::Int(port));
        assert!(decode(&value).is_err());
    }
}

#[test]
fn announce_peer_implied_port_accepts_zero_and_one_and_omits_false() {
    for (implied_port, encoded) in [(false, 0), (true, 1)] {
        let mut value = assert_roundtrip(&query(WireQuery::AnnouncePeer {
            arguments: Map(AnnouncePeerArguments {
                id: ID,
                info_hash: TARGET,
                port: 6881,
                implied_port: OptionalBool(implied_port),
                token: ByteString(vec![]),
            }),
        }));
        let arguments = argument_dictionary(&mut value, b"a");
        assert_eq!(
            arguments.get(b"implied_port".as_slice()),
            implied_port.then_some(&Value::Int(1))
        );
        arguments.insert(b"implied_port".to_vec(), Value::Int(encoded));
        let message = decode(&value).unwrap();
        let WireKind::Query {
            query: WireQuery::AnnouncePeer { arguments },
            ..
        } = &message.kind
        else {
            panic!("expected announce_peer");
        };
        assert_eq!(arguments.0.implied_port.0, implied_port);
        let mut serialized = assert_roundtrip(&message);
        let arguments = argument_dictionary(&mut serialized, b"a");
        assert_eq!(
            arguments.get(b"implied_port".as_slice()),
            implied_port.then_some(&Value::Int(1))
        );
    }
}
