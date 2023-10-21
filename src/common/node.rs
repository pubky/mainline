//! Struct and implementation of the Node entry in the Kademlia routing table
use std::net::IpAddr;

use crate::common::Id;

#[derive(Debug, Clone, PartialEq)]
/// Node entry in Kademlia routing table
pub struct Node {
    pub id: Id,
    pub ip: IpAddr,
    pub port: u16,
}
