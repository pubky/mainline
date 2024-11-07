# Censorship Resistance

## Overview

One of the main criticism against distributed hash tables are their susceptibility to Sybil attacks,
and by extension censorship. This document is an overview over the problem and how this implementation minimizes this risk.

[Real-World Sybil Attacks in BitTorrent Mainline DHT](https://www.cl.cam.ac.uk/~lw525/publications/security.pdf) paper divides Sybil attacks 
into “horizontal”, and “vertical”, the former tries to flood the entire network with Sybil nodes, while the later tries to target specific region of
the ID space, to censor specific info-hashes.

Our strategy in this document is to first: explain how can we transform all vertical attacks to horizontal attacks by necessity, and second: explore the
cost of such horizontal attacks and the cost of resisting such attacks, and we consider the system resistant to censorship, if the cost of resistance to
horizontal Sybil attacks are much lower than the cost of sustaining such attacks for extended periods of time.

### Non goals

For the sake of this document we will NOT discuss extreme forms of censorship like filtering out UDP packets that look like Bittorrent messages at the ISP level.
Or filtering out packets that includes specific info hashes. This form of censorship apply to more than just DHTs, including DNS queries and more. And are better
handled using VPNs and other firewall circumvention solutions. Including HTTPs relays that are hard to filter out or predict their purpose.

We will focus on how to keep DHTs resistant to vulnerabilities that are inherint to their nature as open networks without a central reputation auhtority.

Similarly, we will not discuss the effect of Sybil attacks on privacy, if one wants to keep their queries private, they are also advised to use a VPN or a trusted HTTPs server to relay their queries.

## Vertical Sybil Attacks

In a DHT, to store or read a piece of information, you need to lookup the closest `k` (usually 20) nodes to that info hash using XOR metric defined in [BEP_0005](https://www.bittorrent.org/beps/bep_0005.html).

A vertical Sybil attack is an attack where a malicious actor runs enough nodes close to an info hash that a writer or reader only writes or reads to and from the
attacker nodes, which in turn censors the information that it doesn't want to make available to the network.

A vertical Sybil attack is defeated, if at least one honest node is among the closest `k` nodes to an info hash (target).

Our solution is to not choose the closest `k` nodes to publish our data to, but instead we choose all the nodes that are closer to the target than a distance `dk` 
which is the distance of the `k` node given a specific size of the DHT.

Why? because if nodes are distributed evenly (uniformly) across the ID space (from 0 to the max ID) then the closest 20th node should be at a distance around 
`20 * (max ID / number of nodes)`. If instead we found 40 nodes before that `dk` then surely some of these nodes if not half of them are malicious Sybil nodes,
so if we publish to the 40, we make sure that some honest nodes got the information we need to publish.

This strategy requires two things:

1. Enforcing a uniform distribution of nodes (nodes shouldn't have the liberty to choose where to land on the ID space), which is solved using [BEP_0042](https://www.bittorrent.org/beps/bep_0042.html).
2. An accurate and fresh estimation of the DHT size, which we discuss how we obtain efficiently [here](./dht_size_estimate.md).

An attacker then can't fool a node to only write to its upnormally close nodes to the target, unless it changes the size of the netire network, aka: horizontal Sybil attack.

## Horizontal Sybil Attacks

So if an attacker can't perform a vertical Sybil attack, it has to run > 20 times the number of current honest nodes to have a good chance of taking over an info hash,
i.e being in control of all 20 closest nodes to a target.

Firstly, because we have a good way to estimate the dht size, we can all see the DHT size suddenly increasing 20x, which at least gives us all a chance to react to such extreme attack.

Secondly, because of [BEP_0042](https://www.bittorrent.org/beps/bep_0042.html), an IPv4 can't have any more than 8 nodes, so an attacker needs to at least have control of millions of IP addresses.

Thirdly, the current DHT size estimate seems to be near the limits enforced by [BEP_0042](https://www.bittorrent.org/beps/bep_0042.html) (~10 million nodes), which means an attacker will
need to create more than 9 million nodes and try to replace already running nodes with their Sybil nodes, except that [BEP_0005](https://www.bittorrent.org/beps/bep_0005.html) favors older nodes
than newer ones.

To summarize, an attacker needs to have control over millions of IP addresses, actually run millions of nodes, hope that existing nodes churn enough to give them a chance to replace them in nodes routing tables,
and hope that no one notices or reacts to such attack, and even then they need to sustain that attack, because as soon as they give up, the network resumes its normal operation.

It is safe to say that much simpler modes of censorship are much more likely to be employed instead.

## Conclusion

While theoritically DHTs are not immune to Sybil nodes, and while it is impossible to stop attempts to inject nodes all over the DHT to snoop on traffic, it is not at all easy or practical to
disrupt the operation of a large DHT network.

The security of a DHT thus boils down to the number of honest nodes, as long as we don't see a massive decline of the size of the DHT, Mainline will remain as unstopable as a network based on
the Internet can be.
