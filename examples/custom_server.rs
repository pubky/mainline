use std::{thread::sleep, time::Duration};

use mainline::{
    rpc::messages::MessageType,
    server::{DefaultServer, Server},
    Dht, RoutingTable,
};
use tracing::{info, instrument, Level};

#[derive(Debug, Default)]
struct MyCustomServer {
    inner: DefaultServer,
}

impl Server for MyCustomServer {
    #[instrument]
    fn handle_request(
        &mut self,
        routing_table: &RoutingTable,
        from: std::net::SocketAddr,
        request: &mainline::rpc::messages::RequestSpecific,
    ) -> MessageType {
        info!(?request, ?from, "Request from");

        // Do something ...
        // For example, rate limiting:
        // if self.rate_limiter.check() { return };

        self.inner.handle_request(routing_table, from, request)
    }
}

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    Dht::builder()
        .custom_server(Box::<MyCustomServer>::default())
        .build()
        .unwrap();

    sleep(Duration::from_secs(5))
}
