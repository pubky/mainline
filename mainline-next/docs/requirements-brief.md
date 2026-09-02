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

## Protocol Security

Implement and enforce [BEP 42](https://www.bittorrent.org/beps/bep_0042.html)
for IPv4. Generate the local node ID from the public IP, exclude non-compliant
remote nodes from lookup completion and storage eligibility, and retain the
specification's exemptions for local addresses. IPv6 is intentionally outside
this refactor: proper support requires IPv6 networking and traversal, BEP 32
wire fields, and BEP 42 IPv6 node IDs as one complete feature.

## Layered API

Expose low-level mutable GET and PUT streams and implement high-level estimates
and conclusions only by consuming their public events and profile metadata.
The reactor drives networking; polling releases bounded stream capacity, and
dropping a stream cancels its query. Terminal events are last.

## Mutable GET

Expose validated responses, rejections, closest-set progress, timing, and
completion. The high-level stream reports estimate and evidence changes as
`Searching`, `Converged`, `NotFound`, or `Inconclusive`, using profile-relative
coverage, adaptive settling, and deterministic equal-sequence tie-breaking.
This lets callers return early once confidence is sufficient.

## Mutable PUT

Do not expose or send CAS. Expose acknowledgements and untrusted `301`/`302`
claims, verify possible conflicts with bounded GETs, and return a valid newer
item when found while preserving partial writes. Publication conclusions are
derived by the high-level adapter, while exposed evidence lets callers apply
stricter durability policy.
