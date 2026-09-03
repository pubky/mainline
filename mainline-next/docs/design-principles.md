# Design Principles

1. **Bounded and fair**
   Bound resources and use backpressure and cooperative scheduling.

2. **Adaptive and scale-relative**
   Low-level code should avoid hard-coded tuning values except protocol rules
   and safety limits. High-level policy should use few parameters and prefer
   fractions or percentages over fixed counts.

3. **Layered**
   Low-level APIs expose validated events; high-level APIs derive policy and
   conclusions from them.

4. **Observable**
   Expose health and query evidence so callers can evaluate confidence and
   return early.

5. **Runtime-independent**
   Use a library-owned Mio reactor without requiring an application async
   runtime.

6. **Reciprocal participation**
   Make secure server mode easy to enable and encourage reliable, reachable
   clients to contribute capacity to the DHT they benefit from. Becoming a
   server remains an explicit choice and requires the necessary reachability,
   validation, storage, and abuse controls.
