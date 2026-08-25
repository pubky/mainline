## Problem statement

The current architecture does not provide reliable behavior or sufficient
diagnostics under concurrency and poor network conditions:

- Concurrent queries can produce UDP responses faster than they are processed,
  causing packet loss and incomplete results
  ([\#38](https://github.com/pubky/mainline/issues/38)).
- Mutable PUT results can hide evidence that some nodes hold newer data
  ([\#113](https://github.com/pubky/mainline/issues/113)).
- Applications cannot clearly distinguish missing data from failed bootstrap,
  timeouts, or poor DHT connectivity
  ([\#104](https://github.com/pubky/mainline/issues/104)).
- GET queries do not provide a simple way for consumers to know when the best
  observed result has **sufficient confidence** to return early.

The refactor should provide bounded and fair query execution, preserve important
response evidence, expose bootstrap and network health, and report GET
confidence as the query progresses. This should reduce packet loss, improve
mutable-item correctness, and let applications balance latency against
confidence.
