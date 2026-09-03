# Design Principles

1. **Secure by default**
   Treat network input, remote nodes, bootstrap sources, tokens, and peer claims
   as untrusted. Validate them before they affect results, identity, routing, or
   storage. Bound amplification and resource abuse, and ship serving only with
   complete safety controls.

2. **Bounded and fair**
   Bound resource use, apply backpressure, and schedule work fairly and
   cooperatively.

3. **Runtime-independent**
   Drive network progress with a library-owned Mio reactor. Keep public futures
   and streams independent of any particular async runtime.

4. **Layered**
   Low-level APIs expose validated results, rejections, and progress. High-level
   APIs derive policy and conclusions only from them.

5. **Observable**
   Expose health and query evidence so callers can assess confidence and decide
   when to return.

6. **Adaptive and scale-relative**
   Avoid fixed low-level tuning except for protocol rules and safety limits. Use
   few high-level policy parameters, preferring relative measures to fixed
   counts.

7. **Reciprocal participation**
   Encourage clients to contribute by making secure server mode easy to enable.
   Require explicit operator opt-in; after opt-in, activate serving only with
   verified inbound reachability, sufficient configured resources for bounded
   storage, and active validation and abuse controls.
