# KRPC Wire Types

[`src/wire/`](../src/wire/mod.rs) represents IPv4 KRPC messages independently of
sockets and domain policy. Successful deserialization establishes field shapes,
not whether a message is valid for a particular operation or safe to act on.

## Design choices

Custom byte and dictionary wrappers enforce bencoding representations that
Serde convenience types do not: `serde_bytes` also accepts integer lists, and
derived structs can accept positional lists. Binary strings remain opaque.

Unknown fields are ignored for forward compatibility, including fields defined
for another message kind. Recognized fields still undergo representation
checks. The [implementation comparison](wire-comparison.md) records the accepted
compatibility choices and distinguishes them from protocol requirements.

Responses do not identify their originating query, so fields remain independent
for query-aware validation. This preserves absent versus empty contents and
avoids losing fields through variant fallback. Conditional BEP 44 GET replies
may contain only a sequence, but any supplied value must still be retained.

Mutable PUT fields remain independent; the layer above validates their
combinations.

## Scope

The layer above must validate messages before they affect results, routing, or
storage. Its responsibilities include:

- Response requirements for the originating query, such as a token and nodes or
  peers for `get_peers`.
- Mutable-field consistency, salt/encoded-item sizes, signatures, target hashes,
  and sequence/CAS rules. Fixed byte lengths do not establish cryptographic validity.
- Domain conversions, ID/IP binding checks, outgoing read-only policy, and
  human-readable error handling.

Packet/nesting limits, complete-input validation, and general duplicate-key
handling remain future codec work.

**TODO, deferred:** validate canonical bencoding before conversion to `Value`.
`serde_bencode` stores dictionaries in a `HashMap`, losing key order and
overwriting duplicates. Re-encoding normalizes the data; it cannot validate the
original encoding. [BEP 44](https://www.bittorrent.org/beps/bep_0044.html) requires
rejecting invalid item bencoding, including unsorted keys, before accepting a PUT.

The backend requires UTF-8 envelope field names, including ignored extensions;
item dictionary keys and byte-string values may be binary. Revisit this
restriction when choosing the final codec backend.
