# Wire implementation comparison

All differences below are accepted. Neither implementation is uniformly stricter.
The existing [message decoder](../../src/common/messages.rs) includes domain
conversions; the new [wire types](../src/wire/mod.rs) cover Serde representation
only, with validation responsibilities described below.

## Accepted inputs

| Area | Existing implementation | New wire types |
|---|---|---|
| Transaction ID and version | Exactly four bytes each | Arbitrary byte-string lengths |
| Query `ro` | Any `i32`; positive means true | `0`, `1`, or absence |
| `implied_port` | `0..=255`; nonzero means true | `0`, `1`, or absence |
| Arguments | Dictionaries or positional lists | Dictionaries only |
| Byte-string fields | Also coerces integer lists through `serde_bytes` | Byte strings only |
| PUT `target` | Required and emitted | Ignored on input; omitted on output |
| PUT value | Byte strings, including integer-list coercion | Any bencoded value |
| Mutable PUT fields | Checks key/signature lengths and field combinations | Checks lengths; preserves independent fields |
| Errors | `i32` code, UTF-8 description | `i64` code, binary description |

Both require a transaction ID; version is optional. [BEP 5] uses an opaque
transaction string and recommends a four-byte version. Accepting other version
lengths is a compatibility choice. [BEP 43] defines `ro=1`; accepting `0` and
rejecting other values are our parsing choices.

The dictionary, byte-string, and `implied_port` representations match [BEP 5].
[BEP 44] permits strings, integers, lists, and dictionaries as item values. Its
PUT format omits `target`, which the receiver derives from the item; ignoring an
extra incoming `target` is our compatibility choice. Consequently, the existing
decoder rejects PUTs emitted by the new types.

Both enforce [BEP 44]'s 32-byte key and 64-byte signature lengths. The old
conversion also requires `sig` and `seq` with `k`, and rejects `sig`, `seq`,
`salt`, or `cas` without `k`. The new types defer those checks to the layer above.
[BEP 5] errors are integer/string pairs, without an `i32` or UTF-8 restriction;
[BEP 44] adds codes using that same format.

Definitions: [existing wire types](../../src/common/messages/internal.rs),
[new messages](../src/wire/message.rs), [queries](../src/wire/query.rs), and
[flags](../src/wire/optional_bool.rs).

## Response interpretation

The old untagged enum tries variants in order and may discard fields when it
falls back. The new [response structure](../src/wire/response.rs) preserves
recognized fields and rejects malformed representations.

| Input | Existing decoder | New wire types |
|---|---|---|
| `{id, nodes: 1}` | Falls back to Ping | Rejects: `nodes` must be a binary string ([BEP 5]) |
| Duplicate `nodes` | Can fall back to Ping | Rejects under our parsing policy |
| `{id, token, seq, v}` without `k` | Becomes NoMoreRecentValue, losing `v` | Preserves fields for later validation |
| Integer item value | Can fall back to Ping, losing the value | Retains the integer, permitted by [BEP 44] |

The old decoder still rejects some malformed contacts during conversion, such as
a three-byte `nodes` string or a short peer address in a response with a token.
The new types check every supplied contact against [BEP 5]'s six-byte peer and
26-byte node encodings.

Both accept ID-only replies and support `nodes` alongside `values`. ID-only
replies are valid for ping, announce, and PUT, but are not complete GET replies.
[BEP 44] permits conditional GET replies with `seq` but no `k`, `sig`, or `v`;
this does not justify discarding a value actually received. Preserving fields
avoids data loss without declaring every combination valid.

## Encoding and metadata

- **`ro`:** old code emits `0` or `1` on every message kind; new code emits only
  `ro=1`, only on queries, matching [BEP 43].
- **`implied_port`:** old code checks `is_some()`, incorrectly emitting `1` for
  `Some(false)` and `0` for absence. New code omits false and emits `1` for true,
  consistent with [BEP 5]'s optional flag.
- **Field placement:** old code validates `ip` and `ro` on every message kind.
  New queries ignore `ip`; responses/errors ignore `ro`, even when malformed.
  [BEP 42] describes `ip` on responses/errors; [BEP 43] describes `ro` on queries.
  Ignoring them elsewhere is our compatibility choice; neither BEP requires
  rejection there.

## Validation responsibilities

The layer above the wire types will:

- Check mutable PUT field combinations and salt/item sizes before acceptance.
- Validate responses using the originating query and field-consistency rules.
- Handle human-readable error descriptions; wire types preserve received bytes.

The future codec will handle bounds, canonical bencoding, duplicate keys, and
complete-input validation. Both decoders currently accept trailing bytes in the
comparison cases and ignore unknown fields, including repeated unknown fields.
Rejecting duplicate `nodes` is not general duplicate-key validation. These codec
changes remain deferred; see [wire scope](wire.md#scope).

## Verification

Behavior was checked through source inspection and an isolated comparison
program. This document also reflects the subsequent key/signature length checks.

[BEP 5]: https://www.bittorrent.org/beps/bep_0005.html
[BEP 42]: https://www.bittorrent.org/beps/bep_0042.html
[BEP 43]: https://www.bittorrent.org/beps/bep_0043.html
[BEP 44]: https://www.bittorrent.org/beps/bep_0044.html
