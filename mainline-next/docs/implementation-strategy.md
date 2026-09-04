# Implementation Strategy

Develop the new client beside the existing implementation, then replace the old
runtime in one deliberate v9 cutover. Refactoring the current runtime in place
would make it difficult to keep releases working because its actor, socket,
queries, channels, server, and public APIs are tightly coupled.

Make `mainline-next` an unpublished workspace crate while it is under
development. It must not depend on the existing `Dht` or `Rpc`; otherwise their
constraints would leak into the new architecture.

## Component Boundaries

Reuse or port proven leaf code where its behavior remains suitable:

- IDs and BEP 42 primitives;
- mutable and immutable item validation;
- KRPC encoding and decoding; and
- protocol test vectors.

Implement the architecture-specific components anew:

- the Mio reactor and socket ownership;
- admission, scheduling, deadlines, and cancellation;
- lookup and publication state machines;
- bounded event streams and backpressure; and
- bootstrap, health monitoring, and recovery.

Keep the existing `Dht`, `AsyncDht`, server mode, and Testnet helpers operational
during development. The existing server and local Testnets can act as fixtures
for the new client. Apply necessary correctness and security fixes to the old
implementation, but avoid developing both architectures indefinitely.

Do not introduce a shared abstraction merely to eliminate temporary
duplication. Port small, stable protocol components together with their tests;
extract a shared crate only if both implementations will genuinely need it for
a meaningful period.

## Delivery

Implement roadmap stages 1a through 1d in `mainline-next`. Test each stage with
real UDP and local Testnets, and add packet-level compatibility tests where old
and new behavior should match. High-level adapters should also be tested with
synthetic public low-level events.

After stage 1d meets its acceptance criteria, move or promote the new
implementation into the published `mainline` crate, release it as v9, and
remove the old runtime. Do not retain a permanent feature flag that selects
between the two engines; the parallel crate is a migration tool, not a second
supported implementation.
