use colored::*;
use mainline::Dht;
use std::{thread, time::Duration};
use tracing::{debug, info, Level};
use tracing_subscriber::{self, fmt::format::FmtSpan};

/// Custom logger that formats DHT messages
struct DhtLogger;

impl DhtLogger {
    fn info(msg: &str) {
        info!("{}", format_dht_message(msg));
    }

    fn debug(msg: &str) {
        debug!("{}", format_dht_message(msg));
    }
}

/// Format a DHT message for better readability
fn format_dht_message(msg: &str) -> String {
    // Extract message type and content
    if msg.contains("Sending") {
        format_outgoing_message(msg)
    } else if msg.contains("Received") {
        format_incoming_message(msg)
    } else {
        msg.to_string()
    }
}

/// Format outgoing messages
fn format_outgoing_message(msg: &str) -> String {
    format_message(msg, "OUT".bright_blue())
}

/// Format incoming messages
fn format_incoming_message(msg: &str) -> String {
    format_message(msg, "IN".bright_green())
}

/// Common message formatting logic
fn format_message(msg: &str, direction: ColoredString) -> String {
    // Extract message type (find_node, get_peers, etc)
    let msg_type = if msg.contains("find_node") {
        "FIND_NODE".yellow()
    } else if msg.contains("get_peers") {
        "GET_PEERS".purple()
    } else if msg.contains("announce_peer") {
        "ANNOUNCE".cyan()
    } else if msg.contains("ping") {
        "PING".bright_black()
    } else {
        "OTHER".white()
    };

    // Format the message in a structured way
    format!(
        "[{}] {} | {}\n{}",
        direction,
        msg_type,
        "DHT Message".bright_black(),
        format_message_details(msg)
    )
}

/// Format message details in a structured way
fn format_message_details(msg: &str) -> String {
    let mut details = String::new();
    // Extract and format common fields
    if let Some(id) = extract_field(msg, "id=") {
        details.push_str(&format!("  ID: {}\n", id.bright_yellow()));
    }
    if let Some(addr) = extract_field(msg, "address=") {
        details.push_str(&format!("  Address: {}\n", addr.bright_blue()));
    }
    if let Some(nodes) = extract_field(msg, "nodes=") {
        details.push_str(&format!("  Nodes: {}\n", format_nodes(nodes)));
    }
    if let Some(values) = extract_field(msg, "values=") {
        details.push_str(&format!("  Values: {}\n", values.bright_magenta()));
    }
    if let Some(token) = extract_field(msg, "token=") {
        details.push_str(&format!("  Token: {}\n", token.bright_yellow()));
    }
    details
}

/// Extract a field from the message
fn extract_field<'a>(msg: &'a str, field: &str) -> Option<&'a str> {
    msg.split(field)
        .nth(1)
        .map(|s| s.split_whitespace().next().unwrap_or(""))
}

/// Format nodes list for better readability
fn format_nodes(nodes: &str) -> String {
    nodes
        .split(", ")
        .take(3)
        .map(|n| n.bright_cyan().to_string())
        .collect::<Vec<_>>()
        .join(", ")
        + if nodes.split(", ").count() > 3 {
            " ..."
        } else {
            ""
        }
}

fn main() {
    // Configure logging to see ALL details
    tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .with_thread_ids(true)
        .with_target(true)
        .with_file(true)
        .with_ansi(true)
        .with_line_number(true)
        .with_thread_names(true)
        .with_span_events(FmtSpan::FULL)
        .event_format(
            tracing_subscriber::fmt::format()
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_file(true)
                .with_line_number(true)
                .with_target(true)
                .compact(), // This will make the output more compact
        )
        .init();

    // Configure and start the DHT node in server mode
    let dht = Dht::builder()
        .server_mode()
        .build()
        .expect("Failed to create DHT server");

    DhtLogger::info("DHT server node is running! Press Ctrl+C to stop.");
    // Wait for bootstrap to complete
    DhtLogger::info("Waiting for bootstrap...");
    dht.bootstrapped();
    DhtLogger::info("Bootstrap complete!");
    // Keep the program running and show periodic information
    loop {
        thread::sleep(Duration::from_secs(30));
        let info = dht.info();
        // Basic node information
        DhtLogger::info("=== DHT Node Status ===");
        DhtLogger::info(&format!("Node ID: {}", info.id().to_string().yellow()));
        DhtLogger::info(&format!(
            "Local address: {}",
            info.local_addr().to_string().blue()
        ));
        if let Some(addr) = info.public_address() {
            DhtLogger::info(&format!("Public address: {}", addr.to_string().green()));
        }
        DhtLogger::info(&format!(
            "Firewalled: {}",
            if info.firewalled() {
                "Yes".red()
            } else {
                "No".green()
            }
        ));
        DhtLogger::info(&format!(
            "Server mode: {}",
            if info.server_mode() {
                "Yes".green()
            } else {
                "No".yellow()
            }
        ));
        // Network statistics
        let (size_estimate, std_dev) = info.dht_size_estimate();
        DhtLogger::info("=== Network Statistics ===");
        DhtLogger::info(&format!(
            "Estimated nodes in network: {} (±{}%)",
            size_estimate.to_string().cyan(),
            format!("{:.1}", std_dev * 100.0).yellow()
        ));
        DhtLogger::debug(&format!("Raw DHT Info: {:?}", info));
        DhtLogger::info(""); // Blank line to separate updates
    }
}
