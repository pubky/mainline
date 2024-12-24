//! Ipv4 consensus, from votes from the responding nodes
//!
//! Mostly copied from `https://github.com/raptorswing/rustydht-lib/blob/main/src/common/ipv4_addr_src.rs`

use std::net::Ipv4Addr;

const MIN_VOTES: u8 = 10;
const MAX_VOTES: u8 = 20;

#[derive(Clone, Debug)]
struct IPV4Vote {
    ip: Ipv4Addr,
    votes: u8,
}

/// An IPV4Source that takes a certain number of "votes" from other nodes on the network to make its decision.
#[derive(Debug)]
pub struct IPV4Consensus {
    votes: Vec<IPV4Vote>,
}

impl IPV4Consensus {
    pub fn new() -> IPV4Consensus {
        IPV4Consensus { votes: Vec::new() }
    }
}

impl IPV4Consensus {
    pub fn get_best_ipv4(&self) -> Option<Ipv4Addr> {
        let first = self.votes.first();
        match first {
            Some(vote_info) => {
                tracing::debug!(target: "rustydht_lib::IPV4AddrSource", "Best IPv4 address {:?} has {} votes", vote_info.ip, vote_info.votes);

                if vote_info.votes >= MIN_VOTES {
                    Some(vote_info.ip)
                } else {
                    None
                }
            }

            None => None,
        }
    }

    pub fn add_vote(&mut self, proposed_addr: Ipv4Addr) {
        let mut should_sort = false;

        for vote in self.votes.iter_mut() {
            if vote.ip == proposed_addr {
                vote.votes = std::cmp::min(MAX_VOTES, vote.votes + 1);
                should_sort = true;
                break;
            }
        }

        if should_sort {
            self.votes.sort_by(|a, b| b.votes.cmp(&a.votes));
        } else {
            self.votes.push(IPV4Vote {
                ip: proposed_addr,
                votes: 1,
            });
        }
    }

    pub fn decay(&mut self) {
        for vote in self.votes.iter_mut() {
            vote.votes = std::cmp::max(0, vote.votes - 1);
        }

        self.votes.retain(|a| a.votes > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consensus_src() {
        let mut src = IPV4Consensus::new();
        // Nothing yet
        assert_eq!(None, src.get_best_ipv4());

        // One vote, but not enough
        src.add_vote(Ipv4Addr::new(1, 1, 1, 1));
        assert_eq!(None, src.get_best_ipv4());

        // Competing vote, still nothing
        src.add_vote(Ipv4Addr::new(2, 2, 2, 2));
        assert_eq!(None, src.get_best_ipv4());

        // Another 9 votes for the first one. Got something now
        for _ in 0..9 {
            src.add_vote(Ipv4Addr::new(1, 1, 1, 1));
        }
        assert_eq!(Some(Ipv4Addr::new(1, 1, 1, 1)), src.get_best_ipv4());

        // Another 9 votes for the second one. Should still return the first one because in this house our sorts are stable
        for _ in 0..9 {
            src.add_vote(Ipv4Addr::new(2, 2, 2, 2));
        }
        assert_eq!(Some(Ipv4Addr::new(1, 1, 1, 1)), src.get_best_ipv4());

        // Dark horse takes the lead
        src.add_vote(Ipv4Addr::new(2, 2, 2, 2));
        assert_eq!(Some(Ipv4Addr::new(2, 2, 2, 2)), src.get_best_ipv4());

        // Decay happens
        src.decay();

        // Dark horse still winning
        assert_eq!(Some(Ipv4Addr::new(2, 2, 2, 2)), src.get_best_ipv4());

        // Decay happens again
        src.decay();

        // Nobody wins now
        assert_eq!(None, src.get_best_ipv4());
    }
}
