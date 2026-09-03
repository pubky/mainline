# Milestone 1b Requirements: High-Level IPv4 Item API

Milestone 1b builds on the public low-level API from milestone 1a. High-level
mutable adapters must not access the reactor, routing table, or other private
DHT state. Policy should use few parameters and prefer profile-relative
fractions over fixed counts.

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

Provide immutable GET and PUT through the same bounded reactor and query
machinery. Validate values against their targets and publish only with tokens
obtained directly from each destination node for that target.
