use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

pub fn resolve_ipv6(domain: &str) -> Option<Vec<std::net::SocketAddr>> {
    // Google's public DNS server
    let dns_server = "8.8.8.8:53";
    let timeout = Duration::from_secs(3);

    // Create a simple UDP socket
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.set_read_timeout(Some(timeout)).ok()?;
    socket.connect(dns_server).ok()?;

    // Construct a DNS query packet for an AAAA (IPv6) record
    let mut packet = [0u8; 512];
    packet[0] = 0x12; // ID
    packet[1] = 0x34; // ID
    packet[2] = 0x01; // Recursion desired
    packet[5] = 1; // One question

    let mut index = 12;
    let mut split = domain.split(':');
    let domain = split.next().expect("should have domain");
    let port: u16 = split
        .next()
        .and_then(|port| str::parse(port).ok())
        .unwrap_or(0);

    for part in domain.split('.') {
        packet[index] = part.len() as u8;
        index += 1;
        for &b in part.as_bytes() {
            packet[index] = b;
            index += 1;
        }
    }
    packet[index] = 0; // End of domain name
    index += 1;

    // QTYPE (AAAA) and QCLASS (IN)
    packet[index] = 0; // Type (0x001C for AAAA)
    packet[index + 1] = 0x1C;
    packet[index + 2] = 0; // Class (0x0001 for IN)
    packet[index + 3] = 0x01;
    index += 4;

    // Send the query
    socket.send(&packet[..index]).ok()?;

    // Receive the response
    let mut response = [0u8; 512];
    let size = socket.recv(&mut response).ok()?;
    if size < 12 {
        return None; // Response is too short
    }

    // Parse the response (header and answers)
    let mut answers = Vec::new();
    let answer_count = u16::from_be_bytes([response[6], response[7]]);
    let mut offset = index; // Start after the question section

    for _ in 0..answer_count {
        if offset + 12 > size {
            break;
        }
        // Skip name (pointer), type, class
        offset += 10;

        let data_len = u16::from_be_bytes([response[offset], response[offset + 1]]) as usize;
        offset += 2;

        // Check for AAAA record
        if response[offset - 10] == 0 && response[offset - 9] == 0x1C && data_len == 16 {
            let ipv6_addr = std::net::Ipv6Addr::from([
                response[offset],
                response[offset + 1],
                response[offset + 2],
                response[offset + 3],
                response[offset + 4],
                response[offset + 5],
                response[offset + 6],
                response[offset + 7],
                response[offset + 8],
                response[offset + 9],
                response[offset + 10],
                response[offset + 11],
                response[offset + 12],
                response[offset + 13],
                response[offset + 14],
                response[offset + 15],
            ]);
            answers.push(ipv6_addr);
        }
        offset += data_len;
    }

    if answers.is_empty() {
        None
    } else {
        Some(
            answers
                .iter()
                .map(|ip| SocketAddr::from((*ip, port)))
                .collect::<Vec<_>>(),
        )
    }
}
