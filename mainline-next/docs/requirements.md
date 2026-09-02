# Mainline Refactoring Requirements

## Runtime and Resources

- Provide a cloneable async `Dht` driven by a library-owned Mio reactor without
  requiring an application async runtime.
- Allow callers to configure the IPv4 bind address and UDP port. Expose the
  actual bound socket address after initialization.
- The reactor exclusively mutates the socket, routing, transaction, and query
  state; other modules remain independently testable without shared locks.
- Drain UDP to `WouldBlock` and service packets, commands, queries, and deadlines
  cooperatively. Bound all queues and concurrency; a slow consumer pauses only
  its query, and overload becomes backpressure or a typed failure.
- Keep reactor tuning internal and adapt pacing, concurrency, batching, and
  timing to observed network and system load within fixed safety limits.
  Low-level mechanisms avoid policy thresholds and hard-coded tuning values;
  high-level policy uses few parameters and prefers profile-relative fractions.
- Validate transaction IDs and source addresses and safely ignore invalid,
  unmatched, late, duplicate, expired, or spoofed responses.

## Admission and Deadlines

- Admission is cancellation-safe and has its own timeout. The query timeout
  starts after acceptance; an overall deadline includes both phases, while
  per-request deadlines remain separate. Distinguish admission, execution,
  shutdown, outbound-connectivity, and protocol failures.

## Bootstrap, Profiles and Health

- Async construction waits only for reactor and socket initialization. Accept
  both resolved IPv4 addresses and custom DNS name-and-port bootstrap seeds.
  DNS resolution runs outside the caller and reactor threads; bootstrap
  continues independently and does not prevent queries. Surface resolution
  progress and failures through bootstrap health.
- Waiting for bootstrap is optional, and cancelling a waiter does not cancel
  bootstrap.
- Allow callers to replace or extend a profile's bootstrap nodes with addresses
  or DNS names and export candidates for later reuse. Imported nodes and DNS
  results are untrusted seeds and must pass normal response and BEP 42 validation
  before contributing to routing, readiness, or evidence. Export resolved
  addresses only for validated, responsive nodes that remain eligible according
  to routing liveness state; never export DNS names or write tokens.
- Expose the local node ID and socket address, corroborated public-address
  evidence, outbound DHT connectivity, readiness, routing, response and timeout
  rates, bootstrap progress, and partial query outcomes. Outbound connectivity
  is distinct from the inbound reachability needed by server mode. No reachable
  node means unreachable or inconclusive, not `NotFound`; do not guess the cause
  of poor connectivity.
- Support `Mainline` and isolated `Testnet` profiles. Testnet declares a
  non-zero expected node count and its bootstrap nodes and never inherits
  public defaults.
  Low-level operations work with any available node count; profiles affect
  readiness and evidence interpretation, not protocol processing.

## BEP 42 Node IDs

- Implement and enforce the IPv4 requirements of
  [BEP 42](https://www.bittorrent.org/beps/bep_0042.html). This is mandatory
  protocol behavior, not optional high-level policy.
- Generate the local node ID from a configured public IPv4 address or from an
  external-address observation corroborated across independent responses. Do
  not rotate the ID based on one untrusted node. If the public address changes,
  generate a matching ID, reset address-dependent routing state, and bootstrap
  again.
- Allow callers to configure the public IPv4 address used for ID generation.
  Expose whether the active identity is provisional, explicitly configured, or
  based on corroborated observations.
- Validate every remote node ID against the packet's observed source IPv4
  address. A non-compliant node does not count toward lookup termination,
  coverage, readiness, or closest eligible storage nodes, and its token is not
  eligible for PUT. Handling its incoming requests is deferred to server mode.
- Apply BEP 42's private, link-local, and loopback address exemptions so local
  Testnets work without weakening Mainline enforcement. Expose non-compliant
  responses through rejection events and counters.

## Layered Mutable API

- Expose low-level `get_mutable_responses` and `put_mutable_events`; implement
  high-level `get_mutable` and `put_mutable` only over those public streams and
  their profile metadata.
- Low-level streams expose everything required to reproduce high-level results;
  adapters cannot access private DHT state. Terminal reports preserve request,
  response, timeout, protocol-error, acknowledgement, and partial-success data.
  Terminal events are last, and dropping a stream cancels its operation.

## Mutable GET

- Both API levels accept an optional `more_recent_than` sequence and encode it
  as the BEP 44 GET `seq` field. The low-level stream exposes the supplied value
  as operation metadata so an independent high-level adapter can interpret
  `NoMoreRecent` responses.
- The low-level stream exposes every validated response, associated rejection,
  closest-set change, and completion, including node, timing, closer nodes, and
  item or no-value data. Its buffer is bounded and lossless: reserve event and
  terminal capacity before sending, and pause only that query when full.
- The high-level stream emits material estimate or evidence changes as
  `Searching`, `Converged`, `NotFound`, or `Inconclusive`. Evidence includes
  coverage, closest-set state, failures, pending requests, timing, convergence,
  and exact-item support; it is evidence, not a probability.
- Coverage counts valid unique responses from the current closest set, including
  older-item and no-value responses. Exclude duplicates, errors, invalid
  responses, and timeouts; recompute when the closest set changes.
- Use the protocol closest-set width as the Mainline coverage basis and
  `min(protocol width, expected_nodes)` on Testnet. Require at least 40%
  coverage and at most 10% outstanding, rounded conservatively, with no
  absolute floors or caps.
- Select the highest sequence. Resolve equal highest sequences with a stable
  value-then-signature byte tie-breaker, never responder count.
- Derive a bounded settling delay from robust recent RTTs and restart it only
  when the closest set or selected estimate changes. Complete early after
  traversal, coverage, and settling; only then report `Converged`. Insufficient
  final coverage is `Inconclusive`, and hard deadlines win. Report stragglers
  and offer a strict policy that waits for all relevant responses or timeouts.

## Mutable PUT

- High-level PUT neither exposes nor sends CAS. Server-side BEP 44 CAS
  enforcement is deferred to server mode.
- Send a PUT only to a node that directly returned a token in a validated GET
  response for the same target. Associate the opaque token with that node's
  socket address and target; do not transfer it between nodes or targets.
- Use tokens promptly and do not persist them beyond the operation. Treat a
  missing or rejected token as a per-node failure; any token refresh is bounded
  by the existing query deadlines and retry limits.
- The low-level stream exposes lookup progress, acknowledgements, raw `301` and
  `302` claims, bounded verification GETs, and completion. A `302` proves a
  conflict only after a direct GET returns a valid newer item for the target and
  salt. A `301` is a protocol error. Deduplicate and schedule verification
  through normal limits without extra pings.
- Derive `Published`, `Conflict`, or `Inconclusive` only from the stream.
  `Published` means at least one acknowledgement and no verified newer item; it
  is not a durability guarantee. Return a verified newer item for `Conflict`
  and preserve acknowledgements, attempted targets, and target-set size so
  callers can apply stricter policy.

## Immutable Items

- Provide immutable GET and PUT through the same reactor, traversal, admission,
  deadline, validation, token, backpressure, and cancellation machinery.
- Validate an immutable value against its target before exposing it. Send PUT
  only with a token obtained directly from the destination node for that target,
  following the same token lifetime and association rules as mutable PUT.

## Protocol and Lifecycle

- Operate as a BEP 43 read-only node: set `ro=1` on outgoing queries and do not
  answer incoming queries, store data for the network, or generate server
  tokens. Defer all serving behavior to the server-mode milestone.
- Encode only fields valid for each KRPC message.
- Preserve BEP 42 enforcement throughout bootstrap, routing-table updates,
  traversal termination, and mutable and immutable storage-target selection.
- Remove the blocking API and `AsyncDht` duplication. `Dht` clones and active
  streams keep the reactor alive; dropping the last holder wakes, stops, and
  joins it without waiting for network deadlines.
- Treat this as a breaking v9 refactor with native IPv4 as the initial networking
  target. IPv6 is intentionally unsupported rather than partially implemented:
  it requires IPv6 sockets and address types, BEP 32 `nodes6` handling and
  traversal, and BEP 42 IPv6 node-ID generation and validation as one coherent
  feature. Safely ignore unsupported IPv6 contacts without discarding usable
  IPv4 data or misclassifying IPv6-only responses.

## Verification

- Test concurrent UDP load, fairness and bounds, cancellation and deadlines,
  reactor lifetime, source validation, and malformed or spoofed traffic.
- Test BEP 42 generation and validation against its published IPv4 vectors,
  local-address exemptions, non-compliant lookup and storage exclusion,
  corroborated external-address discovery, and ID rotation followed by
  rebootstrap.
- Test degraded Mainline connectivity, one- and two-node Testnets, slow streams,
  GET coverage, ties, settling and stragglers, and partial or conflicting PUTs.
  Test immutable target validation and publication token handling.
  Test bootstrap replacement, extension, DNS resolution and failure, export
  filtering, and reuse without persisting tokens or weakening Testnet isolation.
  Use synthetic events or local Testnets, never the public DHT.
