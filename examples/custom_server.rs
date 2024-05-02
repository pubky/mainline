use std::{thread::sleep, time::Duration};

use mainline::{server::Server, Dht};
use tracing::{info, Level};

#[derive(Debug, Default)]
struct MyCustomServer;

impl Server for MyCustomServer {
    fn handle_request(
        &mut self,
        _rpc: &mut mainline::rpc::Rpc,
        from: std::net::SocketAddr,
        transaction_id: u16,
        request: &mainline::rpc::messages::RequestSpecific,
    ) {
        info!(?request, ?from, ?transaction_id, "INCOMING REQUEST");
        // Do something
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
