use std::net::SocketAddrV4;

use mainline::{
    server::{DefaultServer, MessageType, RequestSpecific, Server},
    Dht, RoutingTable,
};
use tracing::{info, Level};

#[derive(Debug, Default, Clone)]
struct MyCustomServer {
    inner: DefaultServer,
}

impl Server for MyCustomServer {
    fn handle_request(
        &mut self,
        routing_table: &RoutingTable,
        from: SocketAddrV4,
        request: RequestSpecific,
    ) -> (MessageType, Option<Box<[SocketAddrV4]>>) {
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

    let info = client.info().unwrap();

    println!("{:?}", info);

    loop {}
}
