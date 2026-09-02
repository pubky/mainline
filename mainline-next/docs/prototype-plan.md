# Prototype Plan

Build a new, production-shaped prototype to validate the proposed requirements,
design principles, and API against real DHT behavior before replacing the
current implementation.

## Isolation

- Develop the prototype on a dedicated branch.
- Place it in a standalone `mainline-next/` crate at the repository root.
- Keep the current crate working so behavior and resource use can be compared.
- Treat the new prototype as evolutionary: successful parts should be suitable
  for moving into the main crate.

## Scope

Implement one narrow vertical slice at a time:

1. A dedicated Mio reactor with bounded admission and cooperative scheduling.
2. Bootstrap and health reporting, including secure import and export of
   bootstrap nodes, non-blocking custom DNS bootstrap names, configurable IPv4
   binding, and identity and connectivity diagnostics.
3. A low-level mutable GET stream exposing validated network events and
   supporting the optional BEP 44 `more_recent_than` sequence.
4. A high-level GET stream deriving estimates only from the low-level stream.
5. Adaptive settling and evidence-based early completion.
6. Low-level mutable PUT events and high-level publication conclusions.

Use real UDP, KRPC, BEP 42 node-ID enforcement, BEP 44 validation,
routing-table traversal, and discovered nodes for reactor and traversal tests.
Use synthetic public low-level events for deterministic high-level policy
tests. Avoid a prototype-only wire protocol and leave nonessential edge cases
as short TODOs rather than obscuring the architecture.

## Validation

Run the same API against:

- local testnets containing one and two nodes;
- a larger local testnet with slow, silent, invalid, and conflicting nodes;
- optionally, the public DHT for manual, read-only observation, never as an
  acceptance or CI dependency;
- slow consumers, full admission queues, packet bursts, and query cancellation;
- cancellation and expiry during admission, execution, per-request, and overall
  deadline phases;
- replacement, extension, DNS resolution and failure, filtering, persistence,
  and reuse of bootstrap nodes;
- configured and ephemeral ports, explicit and discovered public IPv4
  identities, and outbound-connectivity transitions.

Instrument reactor load, queue occupancy, packet loss, response latency,
timeouts, traversal progress, coverage, and settling decisions.

## Success Criteria

The prototype succeeds when it demonstrates that:

- memory and queues remain bounded under load;
- queries progress fairly and slow consumers apply backpressure;
- dropping a stream cancels its query and the last owner stops the reactor;
- low-level streams expose enough information to implement high-level policy;
- the high-level implementation depends only on the public low-level API;
- callers can return early using understandable evidence;
- BEP 42 local IDs, remote-node eligibility, local-address exemptions, and
  public-address changes behave as specified;
- GET coverage, settling, strict mode, deadlines, and equal-sequence
  tie-breaking behave as specified;
- PUT `301` is a protocol error, and `302` becomes a conflict only after a
  direct GET verifies a newer item;
- one-node and two-node testnets remain useful without weakening mainnet policy;
- poor connectivity is visible through health and terminal query evidence;
- bootstrap exports contain only validated, responsive routing candidates and
  never write tokens; and
- tuning adapts to available CPU and network capacity without unnecessary
  protocol-independent fixed thresholds.

## Integration

Review each vertical slice against `problem-statement.md`,
`design-principles.md`, and `requirements.md`. Record any requirement that is
ambiguous, impractical, or missing before changing the proposal.

After the prototype meets the success criteria, migrate its components into
the main crate incrementally. Do not replace the existing implementation in a
single step.
