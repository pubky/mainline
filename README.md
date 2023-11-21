# Mainline

WIP mainline rust implementation.

For the foreseeable future, this library is limited to the scope of [read-only](https://www.bittorrent.org/beps/bep_0043.html) DHT clients.

- [x] BEP0005 basic
  - [x] Ping
    - [x] Request
    - [x] Response
  - [x] Find node
    - [x] Request
    - [x] Response
  - [x] query (incrementally get closer nodes)
  - [x] Announce/Get peers
    - [x] Get
    - [x] Put
- [ ] BEP0044 arbitrary storage
  - [ ] Mutable data
    - [x] Get
    - [ ] Put
  - [x] Immutable data
    - [x] Get
    - [x] Put
- [x] BEP0043 read-only
  - [x] Does not handle incoming requests if read-only
  - [x] Inform other nodes that this node is read-only
  - [x] Does not add read-only nodes to local routing tables
- [ ] Features for long living clients
  - [ ] API for storing and load routing table between sessions.
  - [ ] Refresh nodes in the local rounting table frequently enough
  - [ ] Control number of concurrent udp requests
  - [ ] Refresh cached queries to keep closest nodes fresh, skipping get before put
  - [ ] Backoff while republishing.
- [ ] BEP0042 security extension
- [ ] Features for server side nodes
  - [ ] Respond with relevant errors.
  - [ ] Egress rate limits
  - [ ] Ip rate limits
  - [ ] Error responses
