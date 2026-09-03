# Milestone 1b Prototype Plan

Build the high-level IPv4 item API as adapters over the public milestone 1a API.
The prototype must demonstrate the intended policy and conclusions without
accessing reactor, routing-table, or RPC internals.

## Scope

Implement one narrow vertical slice at a time:

1. A high-level mutable GET stream deriving estimates only from the low-level
   mutable GET stream.
2. Adaptive settling, evidence-based early completion, and strict mode.
3. High-level mutable PUT conclusions derived only from low-level PUT events.
4. Immutable GET and PUT using the milestone 1a reactor, traversal, validation,
   and token machinery.

Use synthetic public low-level events for deterministic high-level policy
tests. Run end-to-end behavior over the real low-level API on local testnets.

## Validation

Exercise:

- GET coverage, settling, strict mode, deadlines, equal-sequence tie-breaking,
  and slow stragglers;
- conditional GET results with item, no-value, and `NoMoreRecent` response
  combinations;
- early completion and cancellation of the underlying low-level stream;
- successful, partially successful, conflicting, and inconclusive mutable PUTs;
- immutable target validation and destination-bound publication tokens; and
- Mainline and one- or two-node Testnet profile interpretation.

Instrument estimate changes, current closest-set coverage, settling decisions,
ignored stragglers, terminal conclusions, and preserved low-level reports.

## Success Criteria

The milestone 1b prototype succeeds when it demonstrates that:

- the high-level implementation depends only on the public milestone 1a API;
- callers can return early using understandable evidence;
- GET coverage, settling, strict mode, deadlines, and equal-sequence
  tie-breaking behave as specified;
- conditional GET distinguishes `NoNewerItem` from `NotFound`, and its terminal
  report preserves the validated response breakdown;
- insufficient connectivity or coverage produces `Inconclusive`, not
  `NotFound`;
- PUT `301` is a protocol error, and `302` becomes `Conflict` only after the
  low-level direct GET verifies a newer item;
- PUT conclusions preserve partial writes and target-set evidence; and
- immutable operations validate targets and use tokens only for the destination
  and target that issued them.

## Integration

Review each adapter against the shared [problem statement](../problem-statement.md),
[design principles](../design-principles.md), and the milestone 1b
[requirements](requirements.md). Record any requirement that is ambiguous,
impractical, or missing before changing the proposal.

After both milestone prototypes meet their success criteria, migrate their
components into the main crate incrementally. Do not replace the existing
implementation in a single step.
