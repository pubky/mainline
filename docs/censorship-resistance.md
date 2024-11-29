# Censorship Resistance

## Overview

One of the main criticism against distributed hash tables are their susceptibility to Sybil attacks,
and by extension censorship. This document is an overview over the problem and how this implementation minimizes this risk.

[Real-World Sybil Attacks in BitTorrent Mainline DHT](https://www.cl.cam.ac.uk/~lw525/publications/security.pdf) paper divides Sybil attacks 
into “horizontal”, and “vertical”, the former tries to flood the entire network with Sybil nodes, while the later tries to target specific regions of
the ID space, to censor specific info-hashes.

Our strategy in this document is to first: explain how can we transform all vertical attacks to horizontal attacks by necessity, and second: explore the
cost of such horizontal attacks and the cost of resisting such attacks, and we consider the system resistant to censorship, if the cost of resistance to
horizontal Sybil attacks are much lower than the cost of sustaining such attacks for extended periods of time.

### Non Goals

For the sake of this document we will NOT discuss extreme forms of censorship like filtering out UDP packets that look like Bittorrent messages at the ISP level.
Or filtering out packets that includes specific info hashes. This form of censorship apply to more than just DHTs, including DNS queries and more. And are better
handled using VPNs and other firewall circumvention solutions. Including HTTPs relays that are hard to filter out or predict their purpose.

We will focus on how to keep DHTs resistant to vulnerabilities that are inherint to their nature as open networks without a central reputation auhtority.

Similarly, we will not discuss the effect of Sybil attacks on privacy, if one wants to keep their queries private, they are advised to use a VPN or a trusted HTTPs server to relay their queries.

## Vertical Sybil Attacks

### Challenge

In a DHT, nodes store a piece of information with a redundancy factor `k` (usually 20), meaning that a node tries to find the 
`k` closest nodes to the info hash using XOR metric defined in [BEP_0005](https://www.bittorrent.org/beps/bep_0005.html) before
storing the data in these nodes.

This static redundancy factor, opens the room for Vertical Sybil attacks is where a malicious actor runs enough nodes close to an info hash 
that a writer only writes to the attacker Sybil nodes, making it easy for that attacker to censors that information from the rest of the network.

Consider the following example, with a Dht of size `8` and `k=2`, drawing nodes at their distances to a given target, should look like this:

```md
             (1)    (2)                  (3)    (4)           (5)           (6)           (7)    (8)       
|------|------|------|------|------|------|------|------|------|------|------|------|------|------|------|
0      1      2      3      4      5      6      7      8      9      10     11     12     13     14     15
```

So, if an attacker injected two (even closer) nodes, that don't match the distribution of the rest of network (Vertical Sybil as opposed to Horizontal Sybil),
then you would expect the example above to look like this instead:

```md
(s1)  (s2)   (1)    (2)                  (3)    (4)           (5)           (6)           (7)    (8)       
|------|------|------|------|------|------|------|------|------|------|------|------|------|------|------|
0      1      2      3      4      5      6      7      8      9      10     11     12     13     14     15
```

As you can see, if we only store data at the closest `k=2` nodes, the data would be only stored within attacker nodes, thus successefully censored.

### Uniform Distribution

The example above and the solution explained next, both assume a uniform distribution of nodes over the ID space,
besides the fact that such distribution can empirically observed, it is also enforced with security extension [BEP_0042](https://www.bittorrent.org/beps/bep_0042.html) 
that limits the number of nodes to 8 for each IP, and uniformly disrtibute these 8 nodes over the entire ID space.

### Solution

To circumvent vertical sybil attack, we make sure to store data to as many of the closest nodes -that responded to our GET query- as necessary
to satisfy both the following requirements:

#### One or more nodes are further from the target than the `expected distance to k (edk)`.

To understand what that means, consider that we have a rough estimation of the DHT size (which we obtain as explained in the 
documentation of the [Dht Size Estimate](./dht_size_estimate.md)), then we can _expect_ that the closest `k` nodes, are going to be
within a range `edk`. For example, continuing the example from above, in a Dht of `8` nodes in a `16` ID space, we can expect
the closest `2` nodes, within distance `4`.

```md
(s1)  (s2)   (1)    (2)   [edk]          (3)    (4)           (5)           (6)           (7)    (8)       
|------|------|------|------|------|------|------|------|------|------|------|------|------|------|------|
0      1      2      3      4      5      6      7      8      9      10     11     12     13     14     15
```

This is similar but a bit more accurate than the average distance of the `k`th nodes from previous queries.

If we store data in all nodes until `edk` (the expected distance of the first 2 nodes in this example), we would store the data at at least 2 honest nodes.

Because the nature of the DHT queries, we should expect to get a response from at least one of these honest nodes as we query closer and closer nodes to the target info hash.

#### Minimum number of unique subnets with 6 bits prefix.

An extreme, and unlikely, but possible way to defeat our `edk` approach to detect vertical sybil attacks, is to DDoS all the honest nodes
and replace them with enough nodes owned by the attacker. 

To find enough nodes to replace the nodes until `edk` the attacker needs ~4 `/8` blocks, or a single `/6` block.

However, we can make this much more expensive, by keeping track of the number of unique `6 bit prefixes` in each GET query response, 
and store data to enough nodes that have enough unique prefixes to match the average from previous queries.

At the time of writing, this usually means the attacker needs to control up to 12 `/6` blocks.

## Extreme Vertical Sybil Attacks

While we are satisfied that this static metrics to circumvent Sybil attacks make them prohibitively expensive, let's consider what 
happens in the very unlikely event that an attacker has enough resources and motivation to brute force them both.

In this case, an attacker acquires a so many Ips in so many subnets that they can both DDoS all the nodes until the expected distance to the 20th node,
and inject at least 20 nodes with as many unique `6 bit` prefix in their IPs as the average of the rest of the network.

Eventually, the writer will notice that reads after writes (GET after PUT, resolving after publishing) doesn't work for them, which can only be explained
by an extreme targeted censorship attack.

Once the writer notices this, they can manuaully start publishing to more and more nodes around the target, for example, instead of publishing to closest 20 nodes,
start publishing to closest 200, or 2000, where readers, without any manual intervention will be likely to find the data as they are approaching the target.

The writer can do that, by making GET queries to random targets that share enough prefix bits with their target, to find more and more nodes around their target, 
then store their data to these responding nodes.

It is unfortunate that the writer needs to react manuaully at all, but given how extreme this attack is, we are satisfied with
the defese being way cheaper than the attack, making the attack not only unsustainable, but also unlikely to happen, given that the attacker knows
it won't be sustainable.

## Horizontal Sybil Attacks

If an attacker can't perform a vertical Sybil attack, it has to run > 20 times the number of current honest nodes to have a good chance of taking over an info hash,
i.e being in control of all 20 closest nodes to a target.

Firstly, because we have a good way to estimate the dht size, we can all see the DHT size suddenly increasing 20x, which at least gives us all a chance to react to such extreme attack.

Secondly, because of BEP_0042, an IPv4 can't have any more than 8 nodes, so an attacker needs to at least have control of millions of IP addresses.

Thirdly, the current DHT size estimate seems to be near the limits enforced by BEP_0042 (~10 million nodes), which means an attacker will
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
