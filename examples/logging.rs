use colored::*;
use mainline::Dht;
use std::{
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tracing::{debug, enabled, info, Level, Subscriber};
use tracing_subscriber::{
    fmt::{format::Writer, FmtContext, FormatEvent, FormatFields},
    registry::LookupSpan,
};

// Constants for common strings
const SOCKET_OUT: &str = "SOCKET OUT";
const SOCKET_IN: &str = "SOCKET IN";
const SOCKET_ERROR: &str = "SOCKET ERROR";

/// Custom formatter for DHT logs
#[derive(Default)]
struct DhtFormatter;

impl<S, N> FormatEvent<S, N> for DhtFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let metadata = event.metadata();
        let level = metadata.level();

        let mut fields = String::with_capacity(256); // Pre-allocate string buffer
        let mut visitor = StringVisitor::new(&mut fields);
        event.record(&mut visitor);

        // Format based on target and content
        let formatted = if metadata.target() == "dht_socket" {
            format_socket_message(&fields)
        } else {
            format_dht_message(&fields)
        };

        // Add level color
        let level_color = match *level {
            Level::TRACE => "TRACE".bright_black(),
            Level::DEBUG => "DEBUG".blue(),
            Level::INFO => "INFO".green(),
            Level::WARN => "WARN".yellow(),
            Level::ERROR => "ERROR".red(),
        };

        // Exibe o timestamp como segundos desde a época Unix
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        writeln!(writer, "{} {} {}", level_color, timestamp, formatted)
    }
}

// Helper to extract message string from event
struct StringVisitor<'a>(&'a mut String);

impl<'a> StringVisitor<'a> {
    fn new(s: &'a mut String) -> Self {
        StringVisitor(s)
    }
}

impl<'a> tracing::field::Visit for StringVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.push_str(field.name());
        self.0.push('=');
        self.0.push_str(&format!("{:?} ", value));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.push_str(field.name());
        self.0.push('=');
        self.0.push_str(value);
        self.0.push(' ');
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0.push_str(field.name());
        self.0.push('=');
        self.0.push_str(&value.to_string());
        self.0.push(' ');
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.push_str(field.name());
        self.0.push('=');
        self.0.push_str(&value.to_string());
        self.0.push(' ');
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0.push_str(field.name());
        self.0.push('=');
        self.0.push_str(&value.to_string());
        self.0.push(' ');
    }
}

/// Custom logger that formats DHT messages
#[derive(Debug)]
struct DhtLogger;

impl DhtLogger {
    #[inline]
    fn info(msg: &str) {
        info!("{}", format_dht_message(msg));
    }

    #[inline]
    fn debug(msg: &str) {
        debug!("{}", format_dht_message(msg));
    }
}

/// Format a DHT message for better readability
fn format_dht_message(msg: &str) -> String {
    if msg.contains("socket_message_sending") || msg.contains("socket_message_receiving") {
        format_socket_message(msg)
    } else if msg.contains("Sending") {
        format_outgoing_message(msg)
    } else if msg.contains("Received") {
        format_incoming_message(msg)
    } else if msg.contains("socket_error") {
        format_socket_error(msg)
    } else if msg.contains("socket_validation") {
        format_socket_validation(msg)
    } else {
        msg.to_string()
    }
}

/// Format socket error messages
fn format_socket_error(msg: &str) -> String {
    let mut details = String::with_capacity(512);

    // Extract error details using the new extractor para capturar erros com espaços completos
    if let Some(error) = extract_error_field(msg, "error=") {
        details.push_str(&format!("  Error: {}\n", error.red()));
    }

    // Extract from address
    if let Some(from) = extract_field(msg, "from=") {
        details.push_str(&format!("  From: {}\n", from.bright_blue()));
    }

    // Extract transaction ID
    if let Some(tid) = extract_field(msg, "transaction_id:") {
        details.push_str(&format!("  Transaction ID: {}\n", tid.bright_yellow()));
    }

    // Extract message content
    if let Some(message) = extract_field(msg, "message=") {
        details.push_str("\n  Raw Message (Hex):\n");
        let hex_dump = message
            .as_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .chunks(16)
            .map(|chunk| chunk.join(" "))
            .collect::<Vec<_>>()
            .join("\n");
        details.push_str(&format!("    {}\n", hex_dump));
    }

    format!("[{}]\n{}", SOCKET_ERROR.red().bold(), details)
}

/// Format socket messages with detailed information
fn format_socket_message(msg: &str) -> String {
    // Determine direction and color
    let (direction, color) = if msg.contains("socket_message_sending") {
        (SOCKET_OUT, "bright_blue")
    } else if msg.contains("socket_message_receiving") {
        (SOCKET_IN, "bright_green")
    } else if msg.contains("socket_error") {
        (SOCKET_ERROR, "red")
    } else {
        (SOCKET_IN, "bright_green")
    };

    let direction = match color {
        "bright_blue" => direction.bright_blue(),
        "bright_green" => direction.bright_green(),
        "red" => direction.red().bold(),
        _ => direction.normal(),
    };

    let mut details = String::with_capacity(512);

    // Special handling for error messages
    if msg.contains("context=socket_error") {
        let mut details = String::new();

        if let Some(error_msg) = extract_field(msg, "message=") {
            // Extract error message parts
            let parts: Vec<&str> = error_msg.split('\n').collect();

            if parts.len() >= 3 {
                // First line contains main error
                let error = parts[0].replace("Failed to parse packet bytes: ", "");
                details.push_str(&format!("  Error Type: {}\n", "Parse Error"));
                details.push_str(&format!("  Details: {}\n", error));

                // Second line contains address
                if let Some(addr) = parts[1].strip_prefix("From: ") {
                    details.push_str(&format!("  Remote Addr: {}\n", addr));
                }

                // Extract transaction ID if present
                if let Some(tid) = extract_field(msg, "transaction_id:") {
                    details.push_str(&format!("  Transaction ID: {}\n", tid.bright_yellow()));
                }

                // Third line contains raw message
                if let Some(raw) = parts[2].strip_prefix("Raw message: ") {
                    details.push_str("\n  Raw Message (Hex):\n");
                    let hex_dump = raw
                        .as_bytes()
                        .iter()
                        .map(|b| format!("{:02x}", b))
                        .collect::<Vec<_>>()
                        .chunks(16)
                        .map(|chunk| chunk.join(" "))
                        .collect::<Vec<_>>()
                        .join("\n");
                    details.push_str(&format!("    {}\n", hex_dump));
                }
            }
        }
        return format!("[{}]\n{}", SOCKET_ERROR.red().bold(), details);
    }

    // Improved message type parsing
    if let Some(msg_type) = msg
        .split("message_type: ")
        .nth(1)
        .or_else(|| msg.split("message_type=").nth(1))
    {
        let type_str = if msg_type.contains("Response(FindNode")
            || msg_type.contains("Response { FindNode")
        {
            "FindNode Response".green()
        } else if msg_type.contains("Response(GetPeers") || msg_type.contains("Response { GetPeers")
        {
            if msg_type.contains("nodes: Some") || msg_type.contains("nodes: [") {
                "GetPeers Response (With Nodes)".green()
            } else {
                "GetPeers Response (No Nodes)".green()
            }
        } else if msg_type.contains("Response(NoValues") || msg_type.contains("Response { NoValues")
        {
            "GetPeers Response (With Nodes)".green()
        } else if msg_type.contains("Response(Ping") || msg_type.contains("Response { Ping") {
            "Ping Response".green()
        } else if msg_type.contains("Request(RequestSpecific") || msg_type.contains("Request {") {
            if msg_type.contains("FindNode") {
                "FindNode Request".yellow()
            } else if msg_type.contains("GetPeers") {
                "GetPeers Request".yellow()
            } else if msg_type.contains("Ping") {
                "Ping Request".yellow()
            } else if msg_type.contains("AnnouncePeer") {
                "AnnouncePeer Request".yellow()
            } else {
                "Unknown Request Type".red()
            }
        } else if msg_type.contains("Error") {
            "Error Response".red()
        } else {
            "Unknown Message Type".red()
        };
        details.push_str(&format!("  Message Type: {}\n", type_str));

        // Extract IDs (both requester and responder)
        if let Some(id) = extract_field(msg_type, "requester_id: Id(")
            .or_else(|| extract_field(msg_type, "requester_id: "))
        {
            details.push_str(&format!("  Requester ID: {}\n", &id[..40].bright_yellow()));
        }
        if let Some(id) = extract_field(msg_type, "responder_id: Id(")
            .or_else(|| extract_field(msg_type, "responder_id: "))
        {
            details.push_str(&format!("  Responder ID: {}\n", &id[..40].bright_yellow()));
        }

        // Extract version if present (format it nicely)
        if let Some(version) = msg
            .split("version: Some([")
            .nth(1)
            .or_else(|| msg.split("version: [").nth(1))
        // Try both formats
        {
            if let Some(version_end) = version.split("])").next() {
                let version_nums: Vec<&str> = version_end
                    .split(',')
                    .map(|s| s.trim()) // Remove whitespace
                    .collect();

                if version_nums.len() >= 4 {
                    let formatted_version = format!(
                        "{}.{}.{}.{}",
                        version_nums[0],
                        version_nums[1],
                        version_nums[2],
                        version_nums[3].trim_end_matches(']') // Remove extra bracket if present
                    );
                    details.push_str(&format!("  Version: {}\n", formatted_version.bright_cyan()));
                }
            }
        }

        // Extract read_only flag
        if let Some(read_only) = extract_field(msg, "read_only: ") {
            details.push_str(&format!("  Read Only: {}\n", read_only.yellow()));
        }

        // Extract token if present
        if let Some(token) = extract_field(msg_type, "token: ") {
            let clean_token = token
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim_end_matches(',')
                .trim();
            details.push_str(&format!("  Token: {}\n", clean_token.bright_yellow()));
        }

        // Extract and format nodes if present
        if msg_type.contains("nodes: [") || msg_type.contains("nodes: Some([") {
            let nodes = format_nodes(msg_type);
            if !nodes.is_empty() {
                details.push_str("  Nodes:\n");
                details.push_str(&nodes);
                details.push('\n');
            }
        }
    }

    // Transaction ID
    if let Some(tid) = extract_field(msg, "transaction_id:") {
        details.push_str(&format!(
            "  Transaction ID: {}\n",
            tid.trim_end_matches(',').bright_yellow()
        ));
    }

    // Context
    if let Some(context) = extract_field(msg, "context=") {
        let context_str = match context {
            "socket_message_sending" => "Sending DHT message",
            "socket_message_receiving" => "Receiving DHT message",
            "socket_error" => "Socket error occurred",
            "socket_validation" => "Socket validation",
            _ => context,
        };
        details.push_str(&format!("  Context: {}\n", context_str.bright_black()));
    }

    // Raw message at the end for debugging
    details.push_str("\n  Raw Log:\n");
    details.push_str(&format!("    {}\n", msg.bright_black()));

    format!("[{}]\n{}", direction, details)
}

/// Format outgoing messages
#[inline]
fn format_outgoing_message(msg: &str) -> String {
    format_message(msg, SOCKET_OUT.bright_blue())
}

/// Format incoming messages
#[inline]
fn format_incoming_message(msg: &str) -> String {
    format_message(msg, SOCKET_IN.bright_green())
}

/// Common message formatting logic
fn format_message(msg: &str, direction: ColoredString) -> String {
    let msg_type = match () {
        _ if msg.contains("find_node") => "FIND_NODE".yellow(),
        _ if msg.contains("get_peers") => "GET_PEERS".purple(),
        _ if msg.contains("announce_peer") => "ANNOUNCE".cyan(),
        _ if msg.contains("ping") => "PING".bright_black(),
        _ => "OTHER".white(),
    };

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
    let mut details = String::with_capacity(256);

    // Extract and format common fields
    let fields = [
        ("id=", "ID", "bright_yellow"),
        ("address=", "Address", "bright_blue"),
        ("nodes=", "Nodes", ""),
        ("values=", "Values", "bright_magenta"),
        ("token=", "Token", "bright_yellow"),
        ("transaction_id=", "Transaction ID", "bright_yellow"),
    ];

    for (field, label, color) in fields.iter() {
        if let Some(value) = extract_field(msg, field) {
            if *field == "nodes=" {
                details.push_str(&format!("  {}:\n{}\n", label, format_nodes(value)));
            } else {
                let colored_value = match *color {
                    "bright_yellow" => value.bright_yellow(),
                    "bright_blue" => value.bright_blue(),
                    "bright_magenta" => value.bright_magenta(),
                    _ => value.normal(),
                };
                details.push_str(&format!("  {}: {}\n", label, colored_value));
            }
        }
    }

    details
}

/// Extract a field from the message
#[inline]
fn extract_field<'a>(msg: &'a str, field: &str) -> Option<&'a str> {
    msg.split(field).nth(1).and_then(|s| {
        let s = s.trim_start();
        if s.starts_with('"') {
            s.trim_start_matches('"').split('"').next()
        } else {
            s.split_whitespace().next()
        }
    })
}

/// Extract an error field from the message
#[inline]
fn extract_error_field<'a>(msg: &'a str, field: &str) -> Option<&'a str> {
    msg.split(field).nth(1).map(|s| {
        let s = s.trim_start();
        // Define delimitadores que indicam o final do valor do campo.
        let delimiters = [" from=", " transaction_id:", " message=", "\n"];
        let mut end = s.len();
        for delim in delimiters.iter() {
            if let Some(idx) = s.find(delim) {
                end = end.min(idx);
            }
        }
        s[..end].trim()
    })
}

/// Format nodes list for better readability
fn format_nodes(msg: &str) -> String {
    msg.split("Node {")
        .skip(1)
        .filter_map(|node_str| {
            let id = extract_field(node_str, "Id(")?;
            let addr = node_str
                .split("address: ")
                .nth(1)?
                .split(',')
                .next()?
                .trim();

            // Extract last_seen if present and clean up any trailing characters
            let last_seen = if let Some(last_seen_str) = node_str.split("last_seen: ").nth(1) {
                if let Some(value) = last_seen_str
                    .split([',', '}', ']', ')']) // Split on any of these characters
                    .next()
                    .map(|s| s.trim())
                {
                    format!(", {}", format!("last_seen: {}", value).bright_magenta())
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            Some(format!(
                "    {} @ {}{}",
                id[..40].bright_yellow(),
                addr.bright_blue(),
                last_seen
            ))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_socket_validation(msg: &str) -> String {
    format!("[{}] {}", "SOCKET VALIDATION".magenta().bold(), msg)
}

fn main() {
    // Configure logging with our custom formatter
    tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .with_thread_ids(true)
        .with_target(true)
        .with_file(true)
        .with_ansi(true)
        .with_line_number(true)
        .event_format(DhtFormatter::default())
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

        // Header
        DhtLogger::info(""); // Blank line to separate
        DhtLogger::info(&format!(
            "    {}",
            "=== DHT Node Status ===".bright_cyan().bold()
        ));

        // Basic node information
        DhtLogger::info(&format!(
            "    Node ID    : {}",
            info.id().to_string().bright_yellow()
        ));

        DhtLogger::info(&format!(
            "    Local Addr : {}",
            info.local_addr().to_string().bright_blue()
        ));

        if let Some(addr) = info.public_address() {
            DhtLogger::info(&format!(
                "    Public Addr: {}",
                addr.to_string().bright_green()
            ));
        }

        DhtLogger::info(&format!(
            "    Firewalled : {}",
            if info.firewalled() {
                "Yes".red().bold()
            } else {
                "No".green().bold()
            }
        ));

        DhtLogger::info(&format!(
            "    Server Mode: {}",
            if info.server_mode() {
                "Active".green().bold()
            } else {
                "Inactive".yellow().bold()
            }
        ));

        // Statistics separator
        DhtLogger::info("");
        DhtLogger::info(&format!(
            "    {}",
            "=== Network Statistics ===".bright_cyan().bold()
        ));

        // Network statistics
        let (size_estimate, std_dev) = info.dht_size_estimate();
        DhtLogger::info(&format!(
            "    Estimated nodes in network: {} (±{}%)",
            size_estimate.to_string().cyan().bold(),
            format!("{:.1}", std_dev * 100.0).yellow()
        ));

        DhtLogger::info(""); // Blank line to separate updates

        // Debug info in compact format
        if enabled!(Level::DEBUG) {
            DhtLogger::debug(&format!("    Raw DHT Info: {:#?}", info));
        }

        DhtLogger::info(""); // Blank line to separate updates
    }
}
