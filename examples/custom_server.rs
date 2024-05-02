use std::{thread::sleep, time::Duration};

use mainline::{
    server::{DhtServer, Server},
    Dht,
};
use tracing::{info, instrument, Level};

#[derive(Debug, Default)]
struct MyCustomServer {
    inner: DhtServer,
}

impl Server for MyCustomServer {
    #[instrument]
    fn handle_request(
        &mut self,
        rpc: &mut mainline::rpc::Rpc,
        from: std::net::SocketAddr,
        transaction_id: u16,
        request: &mainline::rpc::messages::RequestSpecific,
    ) {
        info!(?request, ?from, "Request from");

        // Do something ...
        // For example, rate limiting:
        // if self.rate_limiter.check() { return };

        self.inner
            .handle_request(rpc, from, transaction_id, request)
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
