# Refactoring Roadmap

## 1a. Minimal Safe IPv4 Client and Low-Level API

Build the Mio-based client described by the current proposal:

- bounded and fair query execution;
- bootstrap and health reporting, including custom DNS bootstrap names and
  import and export of bootstrap nodes;
- configurable IPv4 bind address and port, plus an optional public IPv4 address
  for BEP 42 identity generation;
- diagnostics for the local identity and socket, corroborated public address,
  and outbound DHT connectivity;
- low-level mutable GET events, including `more_recent_than`, validated
  responses, rejections, closest-set progress, timing, and completion;
- low-level mutable PUT events, including acknowledgements, raw `301` and `302`
  responses, bounded direct GET verification, and completion;
- IPv4 BEP 42 enforcement;
- transaction, source-address, and BEP 44 validation; and
- client-side acquisition and use of write tokens.

The client is a [BEP 43](https://www.bittorrent.org/beps/bep_0043.html)
read-only DHT node: it sets `ro=1` on outgoing queries and does not answer
incoming queries. Read-only nodes can still GET and publish data; they simply do
not serve the DHT. Basic routing safety, including bounded buckets and accepting
only validated, responsive nodes, is part of this milestone.

This step succeeds when the public low-level streams expose all evidence and
operation metadata needed to implement high-level policy without access to the
reactor, routing table, or other private DHT state.

## 1b. High-Level IPv4 Item API

Implement the high-level API exclusively as adapters over the public low-level
operations from milestone 1a:

- progressive mutable GET estimates and evidence so callers can judge
  confidence;
- adaptive settling and early completion that does not wait for slow
  stragglers;
- `Searching`, `Converged`, `NotFound`, `NoNewerItem`, and `Inconclusive`
  mutable GET outcomes;
- mutable PUT conclusions that treat `301` as a protocol error and report a
  `302` conflict only after a bounded direct GET verifies and returns a newer
  item, while preserving partial writes; and
- immutable GET and PUT APIs using the same reactor, traversal, validation, and
  token machinery.

This step completes the ergonomic minimal safe IPv4 client API.

## 2. Censorship-Resistant IPv4 Client

Harden routing-table admission and eviction, prefer stable responsive nodes,
enforce IP and network-prefix diversity, detect suspicious node concentration,
and adapt publication breadth when the closest set looks unsafe. Expose the
supporting security evidence, including a DHT-size estimate and its uncertainty
if expected-distance policy depends on it, and test the behavior with local
eclipse and vertical-Sybil simulations.

Extend the API to publish to caller-selected nodes. The operation obtains fresh
tokens from those nodes internally; callers do not attach raw tokens to reusable
`Node` values.

## 3. Optional Additional Low-Level APIs

Optionally expose low-level methods for other client operations, such as peer
discovery and peer announcement. These should reuse the same reactor, traversal,
token, validation, and event machinery. Their names and signatures remain to be
designed.

## 4. Secure IPv6 Client Support

Implement complete IPv6 client support: IPv6 sockets and address types,
[BEP 32](https://www.bittorrent.org/beps/bep_0032.html) `nodes6` and `want`
handling, separate IPv4 and IPv6 routing tables, and BEP 42 IPv6 node-ID
generation and remote validation. Maintain independent BEP 42 local IDs and
rotation state for IPv4 and IPv6. If the BEP 42-relevant prefix of the public
IPv6 address changes, regenerate only the IPv6 ID, reset only address-dependent
IPv6 routing state, and bootstrap IPv6 again. Address changes outside that
prefix, such as IPv6 privacy-address rotations, must preserve the existing IPv6
ID and routing state. Report health, coverage, and diversity separately for each
address family.

Either enabled address family can establish a result from sufficient
family-specific traversal and coverage; IPv6-only operation must not depend on
IPv4 evidence. When both are enabled, keep their evidence distinct and do not
treat IPv6 observations as additional IPv4 evidence. Measure IPv6 diversity by
meaningful network prefixes rather than individual addresses, since many IPv6
addresses can belong to one operator or allocation.

## 5. IPv4 Server Mode

IPv4 server mode may be placed behind a `server` Cargo feature, but making a
node eligible to serve remains an explicit runtime choice. After the operator
enables server-capable mode, the library may activate serving automatically only
when its readiness checks pass. Expose the selected operating mode and inbound
reachability separately from outbound DHT connectivity. Provide server
configuration for storage bounds, request filtering, and abuse controls. Ship
server mode together with:

- secure, source-bound, expiring write tokens;
- complete BEP 44 validation and sequence enforcement;
- bounded peer and value stores with expiration and eviction;
- request filtering and per-source-address and global rate limiting;
- response-size and amplification limits;
- controlled overload shedding; and
- inbound-reachability checks before automatically activating serving for an
  explicitly enabled server-capable node.

Server mode must not be released before its token and abuse controls. Client
mode still needs bounded parsing, response correlation, bounded candidate sets,
and cooperative packet-processing budgets, but it does not need server storage,
token generation, or request rate limiting because it does not answer incoming
queries.

## 6. IPv6 Server Support

Extend server mode to IPv6 only after the secure IPv6 client and IPv4 server
milestones are complete. Apply the same token, validation, storage, overload,
and amplification controls to IPv6 traffic. Add rate limiting by meaningful
IPv6 network prefix so clients cannot evade limits by rotating addresses within
one allocation.

Milestones 3, 4, and 5 may proceed independently after the hardened IPv4
client. Milestone 6 depends on milestones 4 and 5.
