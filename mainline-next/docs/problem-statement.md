## Problem statement

The current architecture is unreliable and hard to diagnose under concurrency,
network failures, and hostile traffic:

- Concurrent queries can produce UDP responses faster than they are processed,
  causing packet loss and incomplete results
  ([\#38](https://github.com/pubky/mainline/issues/38)). Admission, active
  queries, and result streams are not consistently bounded or tied to consumer
  lifetimes, so slow or dropped consumers can waste work and disrupt other
  queries.
- Mutable PUT results can hide evidence that some nodes hold newer data
  ([\#113](https://github.com/pubky/mainline/issues/113)).
- Applications cannot distinguish absent data from bootstrap failure, timeout,
  or poor DHT connectivity
  ([\#104](https://github.com/pubky/mainline/issues/104)), nor observe recovery
  when connectivity, the public address, or bootstrap availability changes
  ([\#61](https://github.com/pubky/mainline/issues/61)).
- GET queries do not show when the best observed result has enough evidence for
  an early return.
- Routing and publication lack strong admission and diversity rules and enough
  evidence to detect eclipse or vertical-Sybil influence.
- Serving lacks a safe, explicit lifecycle and can begin without verified
  inbound reachability, sufficient storage resources, or built-in abuse
  controls.
- Protocol rules are not enforced consistently at every trust boundary, as
  illustrated by emitting the BEP 43 read-only flag on responses
  ([\#108](https://github.com/pubky/mainline/issues/108)).
- Public operations can panic or return errors without a precise failure phase
  or cause
  ([\#19](https://github.com/pubky/mainline/issues/19),
  [\#23](https://github.com/pubky/mainline/issues/23),
  [\#47](https://github.com/pubky/mainline/issues/47)).

The refactor should bound, fairly schedule, and cancel query work; preserve
protocol and response evidence; expose health and recovery; return precise typed
failures without panicking; and report progressive GET confidence. It should
strengthen routing diversity and validation and make serving explicit,
observable, resource-aware, and abuse-resistant. This should improve
correctness, resilience, and censorship resistance while letting applications
balance latency against confidence.
