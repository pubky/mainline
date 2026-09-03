# Milestone 1a Requirements: Low-Level IPv4 Client

## Runtime and Resources

Use a library-owned Mio reactor without requiring an application async runtime.
Keep all queues and concurrency bounded, schedule queries fairly, drain UDP
without avoidable packet loss, and adapt internal pacing and timing to observed
load within fixed safety limits. Keep low-level mechanisms free of policy
thresholds and hard-coded tuning values except protocol and safety limits.
Allow callers to configure the IPv4 bind address and UDP port, and expose the
actual bound address.

## Bootstrap and Profiles

Expose bootstrap and health so callers can distinguish readiness, degraded
connectivity, and missing data. Support explicit Mainline and isolated Testnet
profiles; Testnet declares its expected node count and can operate with one or
two nodes without public bootstrap defaults. Allow replacing or extending
bootstrap nodes with resolved addresses or custom DNS name-and-port seeds. DNS
resolution does not block the caller or reactor. Health keeps DNS resolution
and DHT traversal visible together, including cumulative DNS failures during
and after traversal. One failed name does not fail bootstrap if another seed
succeeds; no usable seed produces degraded or unreachable bootstrap, never a
successful lookup result. Export resolved addresses only for validated,
responsive nodes that remain eligible for routing. Imported and resolved
addresses remain untrusted until validated, and write tokens are never
exported. Health also exposes the local identity and socket, corroborated
public-address evidence, and outbound DHT connectivity. Inbound reachability is
a separate server-mode concern. Health remains live after bootstrap: validated
network evidence updates readiness, and bounded maintenance and bootstrap
retries recover stale routing state with adaptive backoff without recreating
`Dht`.

## Protocol Security

Implement and enforce [BEP 42](https://www.bittorrent.org/beps/bep_0042.html)
for IPv4. Generate the local node ID from the public IP, exclude non-compliant
remote nodes from lookup completion and storage eligibility, and retain the
specification's exemptions for local addresses. Accept an optional configured
public IPv4 address and expose whether the identity is provisional, configured,
or based on corroborated observations. Operate as a BEP 43 read-only node: set
`ro=1` on outgoing queries and do not answer incoming queries or maintain server
state. Serving non-compliant senders, generating server tokens, and enforcing
server-side CAS are deferred to server mode. IPv6 is intentionally outside this
milestone: proper support requires IPv6 networking and traversal, BEP 32 wire
fields, and BEP 42 IPv6 node IDs as one complete feature.

## Low-Level Item API

Expose mutable and immutable GET and PUT streams containing all validated
events, operation metadata, progress, and terminal reports required by an
independent policy adapter. The reactor drives networking; polling releases
bounded stream capacity, and dropping a stream cancels its query. Terminal
events are last.

## Mutable GET

Support the optional BEP 44 `more_recent_than` sequence and expose it as stream
metadata. Expose validated responses, rejections, closest-set progress, timing,
and completion. Responses distinguish items, no value, and `NoMoreRecent`.
Terminal reports preserve cumulative counts for each category, while progress
snapshots preserve the current closest-set evidence.

## Mutable PUT

Do not send CAS; defer its server-side enforcement to server mode. Obtain write
tokens directly from each destination for the same target and do not persist or
transfer them. Expose lookup progress, acknowledgements, and untrusted
`301`/`302` claims. Treat `301` as a protocol error and verify a possible `302`
conflict with a bounded direct GET. Preserve verification results, partial
writes, and completion evidence without deriving a publication conclusion.

## Immutable GET and PUT

Expose immutable GET responses, rejections, closest-set progress, and
completion, validating values against their targets before exposure. Expose
immutable PUT lookup progress, acknowledgements, partial failures, and
completion. Keep destination-bound write tokens private and operation-scoped.
A valid value is definitive; absence remains a high-level coverage decision.
