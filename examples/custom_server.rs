use std::net::SocketAddr;

use mainline::{
    rpc::messages::MessageType,
    server::{DefaultServer, Server},
    Dht, RoutingTable,
};
use tracing::{info, Level};

#[derive(Debug, Default)]
struct MyCustomServer {
    inner: DefaultServer,
}

impl Server for MyCustomServer {
    fn handle_request(
        &mut self,
        routing_table: &RoutingTable,
        from: std::net::SocketAddr,
        request: &mainline::rpc::messages::RequestSpecific,
    ) -> (MessageType, Option<Vec<SocketAddr>>) {
        info!(?request, ?from, "Got Request");

        // Do something ...
        // For example, rate limiting:
        // if self.rate_limiter.check() { return };

        self.inner.handle_request(routing_table, from, request)
    }
}

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let client = Dht::builder()
        .server_mode()
        .custom_server(Box::<MyCustomServer>::default())
        .build()
        .unwrap();

    client.bootstrapped().unwrap();
}
