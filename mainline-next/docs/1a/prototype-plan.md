# Milestone 1a Prototype Plan

Build a production-shaped low-level IPv4 client prototype that validates the
milestone 1a requirements and public event API against real DHT behavior before
replacing the current implementation.

## Isolation

- Develop the prototype on a dedicated branch.
- Place it in a standalone `mainline-next/` crate at the repository root.
- Keep the current crate working so behavior and resource use can be compared.
- Treat the prototype as evolutionary: successful parts should be suitable for
  moving into the main crate.

## Scope

Implement one narrow vertical slice at a time:

1. A dedicated Mio reactor with bounded admission and cooperative scheduling.
2. Bootstrap and health reporting, including secure import and export of
   bootstrap nodes, non-blocking custom DNS bootstrap names, configurable IPv4
   binding, and identity and connectivity diagnostics.
3. A low-level mutable GET stream exposing validated network events and
   supporting the optional BEP 44 `more_recent_than` sequence.
4. Low-level mutable PUT events, token acquisition, raw rejection claims, and
   bounded direct GET verification.

Use real UDP, KRPC, BEP 42 node-ID enforcement, BEP 44 validation,
routing-table traversal, and discovered nodes for reactor and traversal tests.
Avoid a prototype-only wire protocol and leave nonessential edge cases as short
TODOs rather than obscuring the architecture.

## Validation

Run the low-level API against:

- local testnets containing one and two nodes;
- a larger local testnet with slow, silent, invalid, and conflicting nodes;
- optionally, the public DHT for manual, read-only observation, never as an
  acceptance or CI dependency;
- slow consumers, full admission queues, packet bursts, and query cancellation;
- cancellation and expiry during admission, execution, per-request, and overall
  deadline phases;
- replacement, extension, DNS resolution and failure, filtering, persistence,
  and reuse of bootstrap nodes;
- overlapping DNS resolution and DHT traversal, with DNS failures remaining
  visible through completion; and
- configured and ephemeral ports, explicit and discovered public IPv4
  identities, and outbound-connectivity transitions.

Instrument reactor load, queue occupancy, packet loss, response latency,
timeouts, traversal progress, closest-set changes, and low-level event-buffer
pressure.

## Success Criteria

The milestone 1a prototype succeeds when it demonstrates that:

- memory and queues remain bounded under load;
- queries progress fairly and slow consumers apply backpressure;
- dropping a stream cancels its query and the last owner stops the reactor;
- low-level streams expose enough public information to implement all milestone
  1b policy;
- BEP 42 local IDs, remote-node eligibility, local-address exemptions, and
  public-address changes behave as specified;
- conditional GET events and reports distinguish items, `NoMoreRecent`, and no
  value without deriving a high-level conclusion;
- PUT `301` is exposed as a protocol error and a `302` claim triggers a bounded
  direct GET whose result remains visible;
- one-node and two-node testnets remain useful without weakening mainnet
  protocol behavior;
- poor connectivity is visible through health and terminal query evidence;
- partial and total DNS failure remain visible throughout bootstrap;
- bootstrap exports contain only validated, responsive routing candidates and
  never write tokens; and
- tuning adapts to available CPU and network capacity without unnecessary
  protocol-independent fixed thresholds.

## Integration

Review each vertical slice against the shared
[problem statement](../problem-statement.md),
[design principles](../design-principles.md), and the milestone 1a
[requirements](requirements.md). Record any requirement that is ambiguous,
impractical, or missing before changing the proposal.

Milestone 1a is complete when an independent adapter can consume only its public
API. Keep the current crate working until the complete refactor is ready to
migrate incrementally.
