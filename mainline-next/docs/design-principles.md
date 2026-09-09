# Design Principles

These principles define durable outcomes and trust boundaries. Concrete
mechanisms belong in the roadmap or architecture decisions unless they are
fundamental to the design.

1. **Secure by default**
   Treat network input, remote nodes, bootstrap sources, tokens, and peer claims
   as untrusted. Validate them before they affect results, identity, routing, or
   storage.

2. **Bounded and fair**
   Bound resource use, apply backpressure, and schedule work fairly and
   cooperatively.

3. **Runtime-independent**
   Keep public futures and streams independent of any particular async runtime.
   The library owns and drives network progress.

4. **Layered**
   Low-level APIs expose validated results, rejections, and progress. High-level
   APIs derive policy and conclusions only from that public evidence.

5. **Observable**
   Expose health and query evidence so callers can assess confidence and decide
   when to return. Report recoverable failures as typed outcomes with a clear
   lifecycle phase and cause; do not panic for network, overload, cancellation,
   or shutdown conditions.

6. **Adaptive and scale-relative**
   Prefer adaptive behavior and relative policy when network or system scale
   varies. Fixed values are acceptable for protocol rules, safety bounds, and
   justified defaults; document and test them.

7. **Encourage safe participation**
   Make secure server participation easy to enable, encouraging a diverse
   network with more capacity to improve security, resilience, and speed.
   Keep serving explicit, bounded, and observable: require operator opt-in,
   verified inbound reachability, sufficient resources, and active validation
   and abuse controls.
