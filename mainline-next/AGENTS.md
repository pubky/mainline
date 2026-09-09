# Mainline Next Guidelines

All implementation, API, architecture, testing, and documentation decisions in
`mainline-next/` must conform to
[`docs/design-principles.md`](docs/design-principles.md).
The complete solution must solve the problems defined in
[`docs/problem-statement.md`](docs/problem-statement.md), and each scoped change
must preserve or advance that goal.

Before making a decision, review the problem statement and applicable design
principles, preserving their intent rather than only their literal wording. If
a proposed change conflicts with a principle or exposes an ambiguity between
principles, do not silently make an exception. Document the conflict and resolve
it in the design documents before proceeding.
