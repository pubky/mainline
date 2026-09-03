# Refactoring Roadmap

## 1a. Core IPv4 Client and Low-Level APIs

Build the client around a library-owned Mio reactor. Public futures and streams
remain independent of any application async runtime. The milestone includes:

- bounded admission, active queries, and event streams, with fair cooperative
  scheduling, per-query stream backpressure, and cancellation on stream drop;
- cooperative UDP draining to `WouldBlock` and adaptive pacing, concurrency,
  and batching within fixed safety bounds, without low-level policy thresholds;
- panic-free public operations with typed errors that distinguish admission,
  execution, deadline, connectivity, protocol, cancellation, and shutdown
  failures;
- bootstrap with custom DNS names and node import/export, plus health reporting
  for bootstrap, identity, sockets, public-address evidence, and connectivity;
- configurable IPv4 bind address, port, and optional public IPv4 address for
  BEP 42 identity generation;
- low-level mutable GET events, including `more_recent_than`, validated
  responses, rejections, closest-set progress, timing, and completion;
- low-level mutable PUT events, including acknowledgements, raw `301` and `302`
  responses, bounded direct GET verification, and completion;
- low-level immutable GET and PUT streams, including validated values,
  no-value responses, closest-set progress, acknowledgements, partial failure,
  and completion;
- IPv4 BEP 42, transaction, source-address, and BEP 44 validation; and
- operation-scoped acquisition and use of write tokens.

The client is a [BEP 43](https://www.bittorrent.org/beps/bep_0043.html)
read-only node: it sets `ro=1` only on outgoing queries, never answers incoming
queries, and can still GET and PUT. Routing uses bounded buckets and admits only
validated, responsive nodes.

This step succeeds when public streams expose the validated events, bounded
rejection reporting, and metadata needed by high-level policy without private
DHT state.

## 1b. High-Level IPv4 Item APIs

Implement the high-level API exclusively as adapters over the public low-level
operations from milestone 1a:

- progressive mutable GET estimates and evidence, using few profile-relative
  policy parameters and adaptive settling to finish without slow stragglers;
- `Searching`, `Converged`, `NotFound`, `NoNewerItem`, and `Inconclusive`
  mutable GET outcomes;
- mutable PUT conclusions that treat `301` as a protocol error and report a
  `302` conflict only after a bounded direct GET verifies and returns a newer
  item, while preserving partial writes; and
- immutable GET and PUT adapters over the public low-level streams: a valid
  value completes GET immediately, absence requires converged traversal with
  sufficient coverage, and PUT preserves acknowledgement and partial-failure
  evidence.

## 2. IPv4 Censorship-Resistance Hardening

Harden routing admission and eviction, prefer stable responsive nodes, enforce
IP and prefix diversity, detect suspicious concentration, and adapt publication
breadth when the closest set looks unsafe. Expose the supporting evidence,
including DHT-size estimates and their uncertainty when used by policy, and
test local eclipse and vertical-Sybil attacks.

Extend the API to publish to caller-selected nodes. The operation obtains fresh
tokens from those nodes internally; callers do not attach raw tokens to reusable
`Node` values.

## 3. Optional Low-Level Client Operations

Optionally expose low-level methods for other client operations, such as peer
discovery and peer announcement. These should reuse the same reactor, traversal,
token, validation, and event machinery.

## 4. IPv4 Server Mode and Abuse Controls

IPv4 server mode may use a `server` Cargo feature, but requires explicit
operator opt-in. Activate serving only after verifying inbound reachability,
configured bounds for every enabled storage class, and initialized validation
and abuse controls. Expose operating mode and inbound reachability separately
from outbound connectivity. Include:

- secure, source-bound, expiring write tokens;
- complete BEP 44 validation and sequence enforcement;
- bounded peer and value stores with expiration and eviction;
- request filtering and per-source-address and global rate limiting;
- response-size and amplification limits;
- controlled overload shedding.

Client mode retains bounded parsing, response correlation, candidate sets, and
packet-processing budgets, but needs no server storage, token generation, or
request rate limiting.

## 5. IPv6 Client Support

Add complete IPv6 client support: IPv6 sockets and address types,
[BEP 32](https://www.bittorrent.org/beps/bep_0032.html) `nodes6` and `want`
handling, separate routing tables, independent BEP 42 local IDs and rotation
state, and remote-ID validation. A change to the BEP 42-relevant IPv6 prefix
rotates only the IPv6 ID, resets its address-dependent routing state, and
rebootstraps IPv6; privacy-address changes within that prefix preserve both.

Report health, coverage, and diversity per address family. Either family can
establish a result independently; IPv6-only operation must not require IPv4.
Measure IPv6 diversity by meaningful prefixes, not individual addresses.

## 6. IPv6 Server Support

Extend server mode to IPv6 only after the secure IPv6 client and IPv4 server
milestones are complete. Apply the same token, validation, storage, overload,
and amplification controls to IPv6 traffic. Add rate limiting by meaningful
IPv6 network prefix so clients cannot evade limits by rotating addresses within
one allocation.

Milestones 3, 4, and 5 may proceed independently after the hardened IPv4
client. Milestone 6 depends on milestones 4 and 5.
