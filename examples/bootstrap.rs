use mainline::Dht;

use tracing::Level;
use tracing_subscriber;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    let client = Dht::client().unwrap();

    client.bootstrapped();

    let info = client.info();

    println!("{:?}", info);
}
