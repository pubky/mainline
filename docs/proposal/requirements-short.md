# Mainline Refactoring Requirements

## Runtime and Resources

- Provide a cloneable async `Dht` driven by a library-owned Mio reactor without
  requiring an application async runtime.
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
  shutdown, reachability, and protocol failures.

## Bootstrap, Profiles and Health

- Async construction waits only for reactor and socket initialization. DNS runs
  outside the caller and reactor threads; bootstrap continues independently and
  does not prevent queries.
- Expose readiness, routing, reachability, response and timeout rates, bootstrap
  progress, and partial query outcomes. No reachable node means unreachable or
  inconclusive, not `NotFound`; do not guess the cause of poor connectivity.
- Support `Mainline` and isolated `Testnet` profiles. Testnet declares a
  non-zero expected node count and its bootstrap nodes and never inherits
  public defaults.
  Low-level operations work with any available node count; profiles affect
  readiness and evidence interpretation, not protocol processing.

## Layered Mutable API

- Expose low-level `get_mutable_responses` and `put_mutable_events`; implement
  high-level `get_mutable` and `put_mutable` only over those public streams and
  their profile metadata.
- Low-level streams expose everything required to reproduce high-level results;
  adapters cannot access private DHT state. Terminal events are last, and
  dropping a stream cancels its operation.

## Mutable GET

- The low-level stream exposes every validated response, associated rejection,
  closest-set change, and completion, including node, timing, closer nodes, and
  item or no-value data. Its buffer is bounded and lossless: reserve event and
  terminal capacity before sending, and pause only that query when full.
- The high-level stream emits material estimate or evidence changes as
  `Searching`, `Converged`, `NotFound`, or `Inconclusive`. Evidence includes
  coverage, closest-set state, failures, pending requests, timing, convergence,
  and exact-item support.
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
  traversal, coverage, and settling; hard deadlines win. Report stragglers and
  offer a strict policy that waits for all relevant responses or timeouts.

## Mutable PUT

- High-level PUT neither exposes nor sends CAS; retain and enforce server-side
  BEP 44 CAS.
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

## Protocol and Lifecycle

- Encode only fields valid for each KRPC message, including correct BEP 43 `ro`
  handling.
- Remove the blocking API and `AsyncDht` duplication. `Dht` clones and active
  streams keep the reactor alive; dropping the last holder wakes, stops, and
  joins it without waiting for network deadlines.
