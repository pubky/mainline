# Mainline

WIP mainline rust implementation.

For the foreseeable future, this library is limited to the scope of [read-only](https://www.bittorrent.org/beps/bep_0043.html) DHT clients.

- [ ] BEP0005 basic
  - [ ] Ping
    - [x] Request
    - [ ] Response
  - [ ] Find node
    - [x] Request
    - [ ] Response
  - [ ] Announce/Get peers
    - [ ] Request
    - [ ] Response
- [ ] BEP0032 ipv6
- [ ] BEP0042 security extension
- [ ] BEP0043 read-only
  - [x] Does not handle incoming requests if read-only
  - [x] Inform other nodes that this node is read-only
  - [ ] Does not add read-only nodes to local routing tables
- [ ] BEP0044 arbitrary storage
  - [ ] Mutable data
    - [ ] Request
    - [ ] Response
  - [ ] Immutable data
    - [ ] Request
    - [ ] Response
