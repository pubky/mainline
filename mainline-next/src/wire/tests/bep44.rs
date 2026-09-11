use super::*;

fn put_arguments() -> PutArguments {
    PutArguments {
        id: ID,
        token: ByteString(b"tok".to_vec()),
        value: Value::Bytes(b"Hello World!".to_vec()),
        key: None,
        signature: None,
        seq: None,
        salt: None,
        cas: None,
    }
}

#[test]
fn get_matches_wire_examples() {
    for (seq, bytes) in [
        (None, b"d1:ad2:id20:abcdefghij01234567896:target20:0123456789abcdefghije1:q3:get1:t2:aa1:y1:qe".as_slice()),
        (Some(42), b"d1:ad2:id20:abcdefghij01234567893:seqi42e6:target20:0123456789abcdefghije1:q3:get1:t2:aa1:y1:qe"),
    ] {
        assert_wire(&query(WireQuery::Get {
            arguments: Map(GetArguments { id: ID, target: TARGET, seq }),
        }), bytes);
    }
}

#[test]
fn get_omits_absent_sequence() {
    let mut value = assert_roundtrip(&query(WireQuery::Get {
        arguments: Map(GetArguments {
            id: ID,
            target: TARGET,
            seq: None,
        }),
    }));
    let arguments = argument_dictionary(&mut value, b"a");
    assert!(!arguments.contains_key(b"seq".as_slice()));
}

#[test]
fn immutable_put_matches_wire_example() {
    assert_wire(
        &query(WireQuery::Put {
            arguments: Map(put_arguments()),
        }),
        b"d1:ad2:id20:abcdefghij01234567895:token3:tok1:v12:Hello World!e1:q3:put1:t2:aa1:y1:qe",
    );
}

#[test]
fn mutable_put_matches_wire_example() {
    let key = ByteArray([255; 32]);
    let signature = ByteArray([128; 64]);
    let message = query(WireQuery::Put {
        arguments: Map(PutArguments {
            key: Some(key),
            signature: Some(signature),
            seq: Some(42),
            salt: Some(ByteString(b"foobar".to_vec())),
            cas: Some(41),
            ..put_arguments()
        }),
    });
    let expected = [
        b"d1:ad3:casi41e2:id20:abcdefghij01234567891:k32:".as_slice(),
        &key.0,
        b"4:salt6:foobar3:seqi42e3:sig64:",
        &signature.0,
        b"5:token3:tok1:v12:Hello World!e1:q3:put1:t2:aa1:y1:qe",
    ]
    .concat();
    assert_wire(&message, &expected);
}

#[test]
fn put_preserves_values_and_optional_mutable_fields() {
    for value in [
        Value::Bytes(vec![0, 255]),
        Value::Int(42),
        Value::List(vec![Value::Int(1)]),
        Value::Dict(HashMap::from([(vec![255], Value::Bytes(vec![]))])),
    ] {
        for mutable in [false, true] {
            let mut encoded = assert_roundtrip(&query(WireQuery::Put {
                arguments: Map(PutArguments {
                    id: ID,
                    token: ByteString(vec![]),
                    value: value.clone(),
                    key: mutable.then_some(ByteArray([1; 32])),
                    signature: mutable.then_some(ByteArray([2; 64])),
                    seq: mutable.then_some(42),
                    salt: mutable.then(|| ByteString(vec![])),
                    cas: mutable.then_some(41),
                }),
            }));
            let arguments = argument_dictionary(&mut encoded, b"a");
            for field in [b"k".as_slice(), b"sig", b"seq", b"salt", b"cas"] {
                assert_eq!(arguments.contains_key(field), mutable);
            }
            assert!(!arguments.contains_key(b"target".as_slice()));
        }
    }
}

#[test]
fn put_rejects_malformed_mutable_fields() {
    let message = query(WireQuery::Put {
        arguments: Map(put_arguments()),
    });
    for (field, invalid) in [
        ("k", Value::Bytes(vec![])),
        ("k", Value::Bytes(vec![0; 31])),
        ("k", Value::Bytes(vec![0; 33])),
        ("k", Value::List(vec![Value::Int(0); 32])),
        ("sig", Value::Bytes(vec![])),
        ("sig", Value::Bytes(vec![0; 63])),
        ("sig", Value::Bytes(vec![0; 65])),
        ("sig", Value::List(vec![Value::Int(0); 64])),
        ("seq", Value::Bytes(b"42".to_vec())),
        ("cas", Value::Bytes(b"41".to_vec())),
        ("salt", Value::Int(1)),
        ("salt", Value::List(vec![Value::Int(0)])),
    ] {
        let mut value = assert_roundtrip(&message);
        argument_dictionary(&mut value, b"a").insert(field.as_bytes().to_vec(), invalid);
        assert!(decode(&value).is_err(), "invalid {field}");
    }
}

#[test]
fn put_preserves_independent_mutable_fields() {
    for arguments in [
        PutArguments {
            key: Some(ByteArray([0; 32])),
            ..put_arguments()
        },
        PutArguments {
            signature: Some(ByteArray([0; 64])),
            ..put_arguments()
        },
        PutArguments {
            seq: Some(-1),
            ..put_arguments()
        },
        PutArguments {
            cas: Some(i64::MAX),
            ..put_arguments()
        },
        PutArguments {
            salt: Some(ByteString(vec![255; 65])),
            ..put_arguments()
        },
    ] {
        assert_roundtrip(&query(WireQuery::Put {
            arguments: Map(arguments),
        }));
    }
}

#[test]
fn get_responses_match_wire_examples() {
    for (value, seq, bytes) in [
        (Some(Value::Bytes(b"Hello World!".to_vec())), None, b"d1:rd2:id20:abcdefghij01234567895:nodes0:5:token3:tok1:v12:Hello World!e1:t2:aa1:y1:re".as_slice()),
        (None, None, b"d1:rd2:id20:abcdefghij01234567895:nodes0:5:token3:toke1:t2:aa1:y1:re"),
        (None, Some(42), b"d1:rd2:id20:abcdefghij01234567895:nodes0:3:seqi42e5:token3:toke1:t2:aa1:y1:re"),
    ] {
        assert_wire(&response(ResponseArguments {
            token: Some(ByteString(b"tok".to_vec())),
            nodes: Some(CompactNodes(vec![])),
            value,
            seq,
            ..response_arguments()
        }), bytes);
    }
}

#[test]
fn mutable_get_response_matches_wire_example() {
    let key = ByteArray([255; 32]);
    let signature = ByteArray([128; 64]);
    let message = response(ResponseArguments {
        token: Some(ByteString(b"tok".to_vec())),
        nodes: Some(CompactNodes(vec![])),
        value: Some(Value::Bytes(b"Hello World!".to_vec())),
        key: Some(key),
        signature: Some(signature),
        seq: Some(42),
        ..response_arguments()
    });
    let expected = [
        b"d1:rd2:id20:abcdefghij01234567891:k32:".as_slice(),
        &key.0,
        b"5:nodes0:3:seqi42e3:sig64:",
        &signature.0,
        b"5:token3:tok1:v12:Hello World!e1:t2:aa1:y1:re",
    ]
    .concat();
    assert_wire(&message, &expected);
}

#[test]
fn response_preserves_bep44_fields() {
    for value in [
        None,
        Some(Value::Bytes(vec![0, 255])),
        Some(Value::Int(42)),
        Some(Value::List(vec![])),
        Some(Value::Dict(HashMap::from([(vec![255], Value::Int(1))]))),
    ] {
        for mutable in [false, true] {
            assert_roundtrip(&response(ResponseArguments {
                token: Some(ByteString(vec![])),
                value: value.clone(),
                key: mutable.then_some(ByteArray([1; 32])),
                signature: mutable.then_some(ByteArray([2; 64])),
                seq: mutable.then_some(42),
                ..response_arguments()
            }));
        }
    }
    assert_roundtrip(&response(ResponseArguments {
        seq: Some(42),
        ..response_arguments()
    }));
}

#[test]
fn response_rejects_malformed_mutable_fields() {
    for (field, invalid) in [
        ("k", Value::Bytes(vec![])),
        ("k", Value::Bytes(vec![0; 31])),
        ("k", Value::Bytes(vec![0; 33])),
        ("k", Value::List(vec![Value::Int(0); 32])),
        ("sig", Value::Bytes(vec![])),
        ("sig", Value::Bytes(vec![0; 63])),
        ("sig", Value::Bytes(vec![0; 65])),
        ("sig", Value::List(vec![Value::Int(0); 64])),
        ("seq", Value::Bytes(b"42".to_vec())),
    ] {
        let mut value = assert_roundtrip(&response(response_arguments()));
        argument_dictionary(&mut value, b"r").insert(field.as_bytes().to_vec(), invalid);
        assert!(decode(&value).is_err(), "invalid {field}");
    }
}

#[test]
fn put_response_matches_wire_example() {
    assert_wire(
        &response(response_arguments()),
        b"d1:rd2:id20:abcdefghij0123456789e1:t2:aa1:y1:re",
    );
}

#[test]
fn errors_match_wire_examples() {
    for (code, bytes) in [
        (
            ErrorCode::VALUE_TOO_LARGE,
            b"d1:eli205e3:bade1:t2:aa1:y1:ee".as_slice(),
        ),
        (
            ErrorCode::INVALID_SIGNATURE,
            b"d1:eli206e3:bade1:t2:aa1:y1:ee",
        ),
        (ErrorCode::SALT_TOO_LARGE, b"d1:eli207e3:bade1:t2:aa1:y1:ee"),
        (ErrorCode::CAS_MISMATCH, b"d1:eli301e3:bade1:t2:aa1:y1:ee"),
        (
            ErrorCode::SEQUENCE_TOO_LOW,
            b"d1:eli302e3:bade1:t2:aa1:y1:ee",
        ),
        (ErrorCode(999), b"d1:eli999e3:bade1:t2:aa1:y1:ee"),
    ] {
        assert_wire(
            &WireMessage {
                kind: WireKind::Error {
                    error: (code, ByteString(b"bad".to_vec())),
                    requester_address: None,
                },
                ..ping()
            },
            bytes,
        );
    }
}
