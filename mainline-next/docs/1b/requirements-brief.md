# Milestone 1b Requirements: High-Level IPv4 Item API

Milestone 1b builds every high-level item operation only on the corresponding
public low-level stream from milestone 1a. Adapters must not access the reactor,
routing table, or other private DHT state. Policy should use few parameters and
prefer profile-relative fractions over fixed counts.

## Mutable GET

Support the BEP 44 `more_recent_than` sequence and derive progressive estimates
and evidence from low-level events. Report `Searching`, `Converged`, `NotFound`,
`NoNewerItem`, or `Inconclusive` using profile-relative coverage, adaptive
settling, and deterministic equal-sequence tie-breaking. `NoNewerItem`
distinguishes a sufficiently covered conditional lookup containing at least one
`NoMoreRecent` response from one reporting no stored item. Preserve terminal
item, no-value, and `NoMoreRecent` counts while using the current closest set
for progressive evidence, allowing callers to return early once confidence is
sufficient.

## Mutable PUT

Derive `Published`, `Conflict`, or `Inconclusive` only from the public low-level
stream. Report a `302` conflict only when milestone 1a's bounded direct GET
verification found a valid newer item, return that item, and preserve partial
writes. Expose enough evidence for callers to apply stricter durability policy.
The high-level API neither exposes nor sends CAS.

## Immutable Items

Implement immutable GET and PUT as thin adapters over the milestone 1a streams.
The first hash-valid value completes GET, while `NotFound` requires converged
traversal with sufficient closest-set coverage and insufficient evidence is
`Inconclusive`. PUT requires at least one acknowledgement and preserves
partial-failure evidence.
