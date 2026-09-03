# Milestone 1b Requirements: High-Level IPv4 Item API

## Dependency and Policy Boundary

- Implement every high-level item operation only over its public milestone 1a
  stream and profile metadata. Adapters cannot access the reactor, routing
  table, RPC internals, or any other private DHT state.
- Use few policy parameters and prefer profile-relative fractions over fixed
  counts. Keep policy in adapters rather than adding thresholds to low-level
  traversal or protocol mechanisms.
- Preserve the low-level stream's cancellation and lifecycle behavior. A
  high-level adapter that completes early drops its underlying stream and
  retains a consistent final evidence snapshot.

## Mutable GET

- Accept an optional `more_recent_than` sequence and pass it to the milestone 1a
  operation. Interpret its public metadata and `NoMoreRecent` events without
  private state.
- Emit material estimate or evidence changes as `Searching`, `Converged`,
  `NotFound`, `NoNewerItem`, or `Inconclusive`. Evidence includes coverage,
  closest-set state, failures, pending requests, timing, convergence, and
  exact-item support; it is evidence, not a probability.
- For a lookup with `more_recent_than`, emit `NoNewerItem` after sufficient
  coverage when no newer item was found and at least one valid response from the
  current closest set was `NoMoreRecent`. Emit `NotFound` only when sufficient
  coverage reports no stored item. Neither state is valid without sufficient
  coverage; otherwise emit `Inconclusive`.
- Include the low-level terminal report's cumulative validated unique item,
  no-value, and `NoMoreRecent` counts. These explain the conclusion but do not
  replace current closest-set coverage in progressive evidence.
- Count coverage from valid unique responses in the current closest set,
  including older-item, `NoMoreRecent`, and no-value responses. Exclude
  duplicates, errors, invalid responses, and timeouts; recompute when the
  closest set changes.
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
- Derive `Published`, `Conflict`, or `Inconclusive` only from the public
  milestone 1a stream. `Published` means at least one acknowledgement and no
  verified newer item; it is not a durability guarantee.
- Report `Conflict` only for a low-level `302` claim whose bounded direct GET
  returned a valid newer item for the target and salt. Return that item and
  preserve acknowledgements, attempted targets, target-set size, and partial
  writes so callers can apply stricter policy. Treat low-level `301` events as
  protocol errors.

## Immutable Items

- Implement immutable GET and PUT as thin adapters over the corresponding
  public milestone 1a streams.
- Return the first value already validated against the target by the low-level
  stream. Report `NotFound` only after traversal converges with sufficient
  closest-set coverage; report `Inconclusive` when the operation ends without a
  value or both conditions.
- Report immutable PUT as published only with at least one acknowledgement.
  Otherwise report it as inconclusive, and always preserve the target,
  acknowledgements, attempted targets, timeouts, and partial failures.

## Verification

- Use synthetic public low-level events to test high-level policy
  deterministically and prove that adapters require no private DHT state.
- Test GET coverage, ties, settling, strict mode, deadlines, stragglers,
  `NoNewerItem` versus `NotFound`, and insufficient evidence.
- Test published, conflicting, and inconclusive PUT conclusions, including
  partial writes and verified newer items.
- Test immutable early success, covered absence, inconclusive absence,
  acknowledgements, partial failure, target validation, and publication token
  handling through local Testnets. Automated acceptance must not depend on the
  public DHT.
