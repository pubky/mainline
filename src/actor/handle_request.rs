use std::net::SocketAddrV4;

use tracing::info;

use crate::common::{
    FindNodeRequestArguments, Id, MessageType, Node, RequestSpecific, RequestTypeSpecific,
    RoutingTable,
};
use crate::core::iterative_query::GetRequestSpecific;

use super::Actor;

impl Actor {
    /// Handle an inbound KRPC request: forward to server, detect NAT traversal,
    /// and rotate our ID if it doesn't match our public IP.
    pub(super) fn handle_request(
        &mut self,
        from: SocketAddrV4,
        transaction_id: u32,
        request_specific: RequestSpecific,
    ) {
        // By default we only add nodes that responds to our requests.
        //
        // This is the only exception; the first node creating the DHT,
        // without this exception, the bootstrapping node's routing table
        // will never be populated.
        if self.bootstrap.is_empty() {
            if let RequestTypeSpecific::FindNode(param) = &request_specific.request_type {
                self.routing_table.add(Node::new(param.target, from));
            }
        }

        let is_ping = matches!(request_specific.request_type, RequestTypeSpecific::Ping);

        if self.server_mode() {
            let server = &mut self.server;

            match server.handle_request(&self.routing_table, from, request_specific) {
                Some(MessageType::Error(error)) => {
                    self.error(from, transaction_id, error);
                }
                Some(MessageType::Response(response)) => {
                    self.response(from, transaction_id, response);
                }
                _ => {}
            };
        }

        if let Some(our_address) = self.public_address {
            if from == our_address && is_ping {
                self.firewalled = false;

                let ipv4 = our_address.ip();

                // Restarting our routing table with new secure Id if necessary.
                if !self.id().is_valid_for_ip(*ipv4) {
                    let new_id = Id::from_ipv4(*ipv4);

                    info!(
                        "Our current id {} is not valid for address {}. Using new id {}",
                        self.id(),
                        our_address,
                        new_id
                    );

                    self.get(
                        GetRequestSpecific::FindNode(FindNodeRequestArguments { target: new_id }),
                        None,
                    );

                    self.routing_table = RoutingTable::new(new_id);
                }
            }
        }
    }
}
