use std::{net::Ipv4Addr, time::Duration};

use crate::server::Server;

use super::{DEFAULT_BOOTSTRAP_NODES, DEFAULT_REQUEST_TIMEOUT};

#[derive(Debug)]
/// Dht Configurations
pub struct Config {
    /// Bootstrap nodes
    ///
    /// Defaults to [DEFAULT_BOOTSTRAP_NODES]
    pub bootstrap: Vec<String>,
    /// Extra bootstrapping nodes to be used alongside
    /// Explicit port to listen on.
    ///
    /// Defaults to None
    pub port: Option<u16>,
    /// UDP socket request timeout duration.
    ///
    /// The longer this duration is, the longer queries take until they are deemeed "done".
    /// The shortet this duration is, the more responses from busy nodes we miss out on,
    /// which affects the accuracy of queries trying to find closest nodes to a target.
    ///
    /// Defaults to [DEFAULT_REQUEST_TIMEOUT]
    pub request_timeout: Duration,
    /// Server to respond to incoming Requests
    ///
    /// Defaults to None, where the [crate::server::DefaultServer] will be used
    /// if the node kept running for `15` minutes with a publicly accessible UDP port.
    pub server: Option<Box<dyn Server>>,
    /// A known external IPv4 address for this node to generate
    /// a secure node Id from according to [BEP_0042](https://www.bittorrent.org/beps/bep_0042.html)
    ///
    /// Defaults to None, where we depend on the consensus of
    /// votes from responding nodes.
    pub external_ip: Option<Ipv4Addr>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bootstrap: DEFAULT_BOOTSTRAP_NODES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            port: None,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            server: None,
            external_ip: None,
        }
    }
}
