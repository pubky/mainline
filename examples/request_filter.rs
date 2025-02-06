use std::net::SocketAddrV4;

use mainline::{Dht, RequestFilter, RequestSpecific, ServerSettings};
use tracing::{info, Level};

#[derive(Debug, Default, Clone)]
struct Filter;

impl RequestFilter for Filter {
    fn allow_request(&self, request: &RequestSpecific, from: SocketAddrV4) -> bool {
        info!(?request, ?from, "Got Request");

        true
    }
}

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let client = Dht::builder()
        .server_mode()
        .server_settings(ServerSettings {
            filter: Box::new(Filter),
            ..Default::default()
        })
        .build()
        .unwrap();

    client.bootstrapped();

    let info = client.info();

    println!("{:?}", info);

    loop {}
}
