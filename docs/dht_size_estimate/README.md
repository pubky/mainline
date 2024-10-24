# Dht Size Estimattion

In order to get an accurate calculation of the Dht size, you should take
as many lookups (at uniformly disrtibuted target) as you can,
and calculate the average of the estimations based on their responding nodes.
    
Consider a Dht with a 4 bit key space.
Then we can map nodes in that keyspace by their distance to a given target of a lookup.

Assuming a random but uniform distribution of nodes (which can be measured independently),
you should see nodes distributed somewhat like this:

```md
            (1)    (2)                  (3)    (4)           (5)           (6)           (7)      (8)       
|------|------|------|------|------|------|------|------|------|------|------|------|------|------|------|
0      1      2      3      4      5      6      7      8      9      10     11     12     13     14     15
```

So if you make a lookup and optained this partial view of the network:
```md
            (1)    (2)                  (3)                                (4)                  (5)       
|------|------|------|------|------|------|------|------|------|------|------|------|------|------|------|
0      1      2      3      4      5      6      7      8      9      10     11     12     13     14     15
```

Note: you see exponentially less further nodes than closer ones, which is what you should expect from how
the routing table works.

Seeing one node at distance (d1=2), suggests that the routing table might contain 8 nodes,
since its full length is 8 times (d1).

Similarily, seeing two nodes at (d2=3), suggests that the routing table might contain ~11
nodes, since the key space is more than (d2).

If we repeat this estimation for as many nodes as the routing table's `k` bucket size,
and take their average, we get a more accurate estimation of the dht.

## Formula

The estimated number of Dht size, at each distance `di`, is `en_i = i * d_max / di` where `i` is the
count of nodes discovered until this distance and `d_max` is the size of the key space.

The final Dht size estimation is the average of `en_1 + en_2 + .. + en_n`

## Simulation

Running this [simulation](./src/main.rs) for 25 million nodes and a after 25 lookups, we observe:

- Mean estimate: 2,341,502 nodes
- Standard deviation: 9%

![dht-25-million-nodes-25-lookup](./plot.png)

## Acknowledgment

This size estimation was based on [A New Method for Estimating P2P Network Size](https://eli.sohl.com/2020/06/05/dht-size-estimation.html#fnref:query-count)
