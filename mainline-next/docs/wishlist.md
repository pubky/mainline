# Wishlist

These items are intentionally outside the committed roadmap. Estimate and
schedule them only after the IPv4 implementation has provided enough evidence
to refine their design.

## IPv6 Client Support

Add complete IPv6 client support: IPv6 sockets and address types,
[BEP 32](https://www.bittorrent.org/beps/bep_0032.html) `nodes6` and `want`
handling, separate routing tables, independent BEP 42 local IDs and rotation
state, and remote-ID validation. A change to the BEP 42-relevant IPv6 prefix
rotates only the IPv6 ID, resets its address-dependent routing state, and
rebootstraps IPv6; privacy-address changes within that prefix preserve both.

Report health, coverage, and diversity per address family. Either family can
establish a result independently; IPv6-only operation must not require IPv4.
Measure IPv6 diversity by meaningful prefixes, not individual addresses.

## IPv6 Server Support

Extend server mode to IPv6 only after the IPv6 client and IPv4 server are
complete. Apply the same token, validation, storage, overload, and amplification
controls to IPv6 traffic. Add rate limiting by meaningful IPv6 network prefix
so clients cannot evade limits by rotating addresses within one allocation.
