# Implementation Strategy

Develop the new client beside the existing implementation. Refactoring the
current runtime in place would make it difficult to keep releases working
because its actor, socket, queries, channels, server, and public APIs are tightly
coupled.

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

Release scope, versioning, and the migration approach remain undecided.
Completing stage 1d does not require a release or removal of the old runtime.
Use implementation and test results to inform those decisions before committing
to a cutover or long-term support for both implementations.
