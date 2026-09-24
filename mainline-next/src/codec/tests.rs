use std::collections::HashMap;

use serde_bencode::value::Value;

use super::bep44_value::{ItemValueError, MAX_BEP44_VALUE_BYTES};
use super::bounds::{MAX_DATAGRAM_BYTES, MAX_NESTING_DEPTH};
use super::*;
use crate::wire::{
    query::PutArguments, ByteArray, ByteString, WireBool, WireKind, WireMap, WireMessage, WireQuery,
};

const PING: &[u8] = b"d1:ad2:id20:abcdefghij0123456789e1:q4:ping1:t2:aa1:y1:qe";

#[test]
fn ping_roundtrips_through_the_codec() {
    let message = decode(PING).unwrap();

    assert_eq!(encode(&message).unwrap(), PING);
}

#[test]
fn binary_transaction_ids_roundtrip() {
    let mut message = decode(PING).unwrap();
    message.transaction_id = ByteString(vec![0, 255, 128]);

    let encoded = encode(&message).unwrap();

    assert_eq!(decode(&encoded).unwrap(), message);
}

#[test]
fn accepts_a_bep_44_value_at_the_nesting_limit() {
    let message = put_with_nested_value(MAX_NESTING_DEPTH - 2);
    let bytes = encode(&message).unwrap();

    assert_eq!(decode(&bytes).unwrap(), message);
}

#[test]
fn rejects_a_bep_44_value_beyond_the_nesting_limit() {
    let message = put_with_nested_value(MAX_NESTING_DEPTH - 1);
    let bytes = serde_bencode::to_bytes(&message).unwrap();

    assert!(matches!(
        decode(&bytes),
        Err(CodecError::NestingTooDeep {
            maximum: MAX_NESTING_DEPTH,
        })
    ));
    assert!(matches!(
        encode(&message),
        Err(CodecError::NestingTooDeep {
            maximum: MAX_NESTING_DEPTH,
        })
    ));
}

#[test]
fn rejects_a_nested_response_value_before_serialization() {
    let mut message = decode(b"d1:rd2:id20:abcdefghij0123456789e1:t2:aa1:y1:re").unwrap();
    let mut value = Value::Bytes(Vec::new());
    for _ in 0..MAX_NESTING_DEPTH - 2 {
        value = Value::List(vec![value]);
    }
    value = Value::Dict(HashMap::from([(b"x".to_vec(), value)]));
    let WireKind::Response { arguments, .. } = &mut message.kind else {
        unreachable!();
    };
    arguments.0.value = Some(value);

    assert!(matches!(
        encode(&message),
        Err(CodecError::NestingTooDeep {
            maximum: MAX_NESTING_DEPTH,
        })
    ));
}

#[test]
fn ignored_fields_cannot_bypass_the_nesting_limit() {
    let accepted = ping_with_unknown_lists(MAX_NESTING_DEPTH - 1);
    assert!(decode(&accepted).is_ok());

    let rejected = ping_with_unknown_lists(MAX_NESTING_DEPTH);
    assert!(matches!(
        decode(&rejected),
        Err(CodecError::NestingTooDeep {
            maximum: MAX_NESTING_DEPTH,
        })
    ));
}

#[test]
fn byte_string_contents_do_not_affect_nesting() {
    let mut message = decode(PING).unwrap();
    message.version = Some(ByteString(
        b"ldie:999999999999999999999999999999999999".to_vec(),
    ));
    let bytes = serde_bencode::to_bytes(&message).unwrap();

    assert_eq!(decode(&bytes).unwrap(), message);
}

#[test]
fn rejects_trailing_bencode_and_non_bencode_data() {
    for trailing in [b"i1e".as_slice(), b"x"] {
        let mut bytes = PING.to_vec();
        bytes.extend_from_slice(trailing);

        assert!(matches!(
            decode(&bytes),
            Err(CodecError::TrailingData { remaining }) if remaining == trailing.len()
        ));
    }
}

#[test]
fn rejects_oversized_input_before_parsing() {
    let bytes = vec![b'x'; MAX_DATAGRAM_BYTES + 1];

    assert!(matches!(
        decode(&bytes),
        Err(CodecError::DatagramTooLarge {
            actual,
            maximum: MAX_DATAGRAM_BYTES,
        }) if actual == bytes.len()
    ));
}

#[test]
fn parses_input_at_the_size_limit() {
    let bytes = vec![b'x'; MAX_DATAGRAM_BYTES];

    assert!(matches!(decode(&bytes), Err(CodecError::Bencode(_))));
}

#[test]
fn rejects_oversized_encoded_messages() {
    let mut message = decode(PING).unwrap();
    message.transaction_id = ByteString(vec![0; MAX_DATAGRAM_BYTES]);

    assert!(matches!(
        encode(&message),
        Err(CodecError::DatagramTooLarge {
            actual,
            maximum: MAX_DATAGRAM_BYTES,
        }) if actual > MAX_DATAGRAM_BYTES
    ));
}

#[test]
fn outgoing_put_uses_the_bep_44_value_limit() {
    let mut message = put_with_nested_value(0);
    let WireKind::Query {
        query: WireQuery::Put { arguments },
        ..
    } = &mut message.kind
    else {
        unreachable!();
    };
    arguments.0.value = Value::Bytes(vec![0; 996]);

    assert!(encode(&message).is_ok());

    let WireKind::Query {
        query: WireQuery::Put { arguments },
        ..
    } = &mut message.kind
    else {
        unreachable!();
    };
    arguments.0.value = Value::Bytes(vec![0; 997]);
    assert!(matches!(
        encode(&message),
        Err(CodecError::Bep44Value(ItemValueError::TooLarge {
            actual,
            maximum: MAX_BEP44_VALUE_BYTES,
        })) if actual == MAX_BEP44_VALUE_BYTES + 1
    ));
}

#[test]
fn outgoing_get_response_uses_the_bep_44_value_limit() {
    let mut message = decode(b"d1:rd2:id20:abcdefghij0123456789e1:t2:aa1:y1:re").unwrap();
    let WireKind::Response { arguments, .. } = &mut message.kind else {
        unreachable!();
    };
    arguments.0.value = Some(Value::Bytes(vec![0; 997]));

    assert!(matches!(
        encode(&message),
        Err(CodecError::Bep44Value(ItemValueError::TooLarge {
            actual,
            maximum: MAX_BEP44_VALUE_BYTES,
        })) if actual == MAX_BEP44_VALUE_BYTES + 1
    ));
}

#[test]
fn encodes_a_packet_at_the_size_limit() {
    let mut message = decode(PING).unwrap();
    let fixed_size = PING.len() - b"2:aa".len();
    let id_length = MAX_DATAGRAM_BYTES - fixed_size - 5; // Four digits and `:`.
    message.transaction_id = ByteString(vec![0; id_length]);

    let bytes = encode(&message).unwrap();
    assert_eq!(bytes.len(), MAX_DATAGRAM_BYTES);
    assert_eq!(decode(&bytes).unwrap(), message);

    message.transaction_id.0.push(0);
    assert!(matches!(
        encode(&message),
        Err(CodecError::DatagramTooLarge { actual, .. }) if actual > MAX_DATAGRAM_BYTES
    ));
}

#[test]
fn malformed_input_returns_a_bencode_error() {
    for bytes in [
        b"".as_slice(),
        b"not bencode",
        b"d1:t2:aa",
        b"10:x",
        b"999999999999999999999999999999999999:x",
    ] {
        assert!(matches!(decode(bytes), Err(CodecError::Bencode(_))));
    }
}

fn put_with_nested_value(depth: usize) -> WireMessage {
    let mut value = Value::Bytes(Vec::new());
    for _ in 0..depth {
        value = Value::List(vec![value]);
    }

    WireMessage {
        transaction_id: ByteString(b"aa".to_vec()),
        version: None,
        kind: WireKind::Query {
            query: WireQuery::Put {
                arguments: WireMap(PutArguments {
                    id: ByteArray(*b"abcdefghij0123456789"),
                    token: ByteString(b"token".to_vec()),
                    value,
                    key: None,
                    signature: None,
                    seq: None,
                    salt: None,
                    cas: None,
                }),
            },
            read_only: WireBool(true),
        },
    }
}

fn ping_with_unknown_lists(depth: usize) -> Vec<u8> {
    let mut bytes = PING[..PING.len() - 1].to_vec();
    bytes.extend_from_slice(b"1:x");
    bytes.extend(std::iter::repeat_n(b'l', depth));
    bytes.extend(std::iter::repeat_n(b'e', depth));
    bytes.push(b'e');
    bytes
}
