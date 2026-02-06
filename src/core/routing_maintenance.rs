//! Routing table maintenance logic.

use std::net::SocketAddrV4;
use std::time::{Duration, Instant};

use crate::common::{Id, RoutingTable};

const REFRESH_TABLE_INTERVAL: Duration = Duration::from_secs(15 * 60);
const PING_TABLE_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Routing table maintenance state
#[derive(Debug)]
pub struct RoutingMaintenance {
    last_table_refresh: Instant,
    last_table_ping: Instant,
}

/// Decisions about routing table maintenance
#[derive(Debug)]
pub struct MaintenanceDecisions {
    /// Whether to populate the routing table (bootstrap)
    pub should_populate: bool,

    /// Whether to ping nodes
    pub should_ping: bool,

    /// Whether to try switching to server mode
    pub should_switch_to_server: bool,

    /// Node IDs to purge from the routing table
    pub nodes_to_purge: Vec<Id>,

    /// Node addresses to ping
    pub nodes_to_ping: Vec<SocketAddrV4>,
}

impl RoutingMaintenance {
    /// Create new routing maintenance tracker
    pub fn new() -> Self {
        RoutingMaintenance {
            last_table_refresh: Instant::now(),
            last_table_ping: Instant::now(),
        }
    }

    /// Determine what maintenance operations should be performed.
    ///
    /// Computes decisions and resets internal timers when intervals elapse.
    pub fn periodic_maintenance_decisions(
        &mut self,
        routing_table: &RoutingTable,
    ) -> MaintenanceDecisions {
        self.periodic_maintenance_decisions_at(Instant::now(), routing_table)
    }

    fn periodic_maintenance_decisions_at(
        &mut self,
        now: Instant,
        routing_table: &RoutingTable,
    ) -> MaintenanceDecisions {
        let refresh_is_due = now.duration_since(self.last_table_refresh) >= REFRESH_TABLE_INTERVAL;
        let ping_is_due = now.duration_since(self.last_table_ping) >= PING_TABLE_INTERVAL;

        let should_populate = routing_table.is_empty() || refresh_is_due;
        let should_switch_to_server = refresh_is_due;

        let (nodes_to_purge, nodes_to_ping) = if ping_is_due {
            self.last_table_ping = now;
            self.purge_and_ping_candidates(routing_table)
        } else {
            (Vec::new(), Vec::new())
        };

        if refresh_is_due {
            self.last_table_refresh = now;
        }

        MaintenanceDecisions {
            should_populate,
            should_ping: ping_is_due,
            should_switch_to_server,
            nodes_to_purge,
            nodes_to_ping,
        }
    }

    /// Determine which nodes to purge and which to ping.
    ///
    /// Pure function - examines routing table and returns decisions.
    fn purge_and_ping_candidates(
        &self,
        routing_table: &RoutingTable,
    ) -> (Vec<Id>, Vec<SocketAddrV4>) {
        let mut to_purge = Vec::with_capacity(routing_table.size());
        let mut to_ping = Vec::with_capacity(routing_table.size());

        for node in routing_table.nodes() {
            if node.is_stale() {
                to_purge.push(*node.id());
            } else if node.should_ping() {
                to_ping.push(node.address());
            }
        }

        (to_purge, to_ping)
    }
}

impl Default for RoutingMaintenance {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crate::common::{Id, RoutingTable};

    use super::{RoutingMaintenance, REFRESH_TABLE_INTERVAL};

    #[test]
    fn empty_table_does_not_reset_refresh_timer() {
        let mut maintenance = RoutingMaintenance::new();
        let routing_table = RoutingTable::new(Id::random());
        let before = maintenance.last_table_refresh;

        let _ = maintenance.periodic_maintenance_decisions(&routing_table);

        assert_eq!(maintenance.last_table_refresh, before);
    }

    #[test]
    fn refresh_due_updates_refresh_timer() {
        let mut maintenance = RoutingMaintenance::new();
        let routing_table = RoutingTable::new(Id::random());
        let before = maintenance.last_table_refresh;

        // Advance time forward past the refresh interval.
        // We add to Instant::now() instead of subtracting, because on Windows
        // Instant can be close to its internal epoch and subtraction overflows.
        let future = Instant::now() + REFRESH_TABLE_INTERVAL + Duration::from_secs(1);

        let decisions = maintenance.periodic_maintenance_decisions_at(future, &routing_table);

        assert!(decisions.should_populate);
        assert!(decisions.should_switch_to_server);
        assert!(maintenance.last_table_refresh > before);
        assert_eq!(maintenance.last_table_refresh, future);
    }
}
