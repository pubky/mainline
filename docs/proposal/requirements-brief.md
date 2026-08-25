# Mainline Refactoring Requirements

## Runtime and Resources

Use a library-owned Mio reactor without requiring an application async runtime.
Keep all queues and concurrency bounded, schedule queries fairly, drain UDP
without avoidable packet loss, and adapt internal pacing and timing to observed
load within fixed safety limits. Keep low-level mechanisms free of policy
thresholds and hard-coded tuning values except protocol and safety limits.
High-level policy should use few parameters and prefer profile-relative
fractions over fixed counts.

## Bootstrap and Profiles

Expose bootstrap and health so callers can distinguish readiness, degraded
connectivity, and missing data. Support explicit Mainline and isolated Testnet
profiles; Testnet declares its expected node count and can operate with one or
two nodes without public bootstrap defaults.

## Layered API

Expose low-level mutable GET and PUT streams and implement high-level estimates
and conclusions only by consuming their public events and profile metadata.
The reactor drives networking; polling releases bounded stream capacity, and
dropping a stream cancels its query. Terminal events are last. See the
[API summary](api-summary.rs).

## Mutable GET

Expose validated responses, rejections, closest-set progress, timing, and
completion. The high-level stream reports estimate and evidence changes as
`Searching`, `Converged`, `NotFound`, or `Inconclusive`, using profile-relative
coverage, adaptive settling, and deterministic equal-sequence tie-breaking.
This lets callers return early once confidence is sufficient.

## Mutable PUT

Do not expose or send CAS. Expose acknowledgements and untrusted `301`/`302`
claims, verify possible conflicts with bounded GETs, and return a valid newer
item when found while preserving partial writes. Publication policy is applied
only by the profile-aware high-level adapter.
