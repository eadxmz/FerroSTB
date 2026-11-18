use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};
use tokio::net::UdpSocket;
use tokio::sync::watch;

pub async fn run_dns_server(
    socket: UdpSocket,
    host_ip: Ipv4Addr,
    ntp_active: bool,
    stb_tx: watch::Sender<Option<IpAddr>>,
) {
    let mut buf = [0u8; 512];
    let mut logged_domains: HashSet<String> = HashSet::new();

    loop {
        let (len, peer) = match socket.recv_from(&mut buf).await {
            Ok(res) => res,
            Err(_) => continue,
        };
        if len < 12 {
            continue;
        }

        if peer.ip() != IpAddr::V4(host_ip) && !peer.ip().is_loopback() {
            let _ = stb_tx.send_if_modified(|curr| {
                if curr.is_none() {
                    *curr = Some(peer.ip());
                    true
                } else {
                    false
                }
            });
        }

        let qname = parse_qname(&buf[12..len]).to_ascii_lowercase();

        if !ntp_active && (qname.contains("ntp") || qname.contains("time.")) {
            continue;
        }

        if should_spoof(&qname) {
            if logged_domains.insert(qname.clone()) {
                println!("[DNS] Spoofed '{}' -> {} (for STB {})", qname, host_ip, peer.ip());
            }
            if let Some(resp) = build_dns_response(&buf[..len], host_ip) {
                let _ = socket.send_to(&resp, peer).await;
            }
        } else {
            if let Some(resp) = build_nxdomain_response(&buf[..len]) {
                let _ = socket.send_to(&resp, peer).await;
            }
        }
    }
}

fn should_spoof(qname: &str) -> bool {
    qname.contains("acs")
        || qname.contains("tr069")
        || qname.contains("iot.jio.com")
        || qname == "api.jio.com"
        || qname == "jiostb.cdn.jio.com"
        || qname.contains("time.")
        || qname.contains("ntp.")
        || qname.contains("gstatic.com")
        || qname.contains("google.com")
        || qname.contains("googleapis.com")
        || qname.contains("android.com")
}

fn parse_qname(mut slice: &[u8]) -> String {
    let mut parts = Vec::new();
    while !slice.is_empty() {
        let l = slice[0] as usize;
        if l == 0 || slice.len() < 1 + l {
            break;
        }
        parts.push(String::from_utf8_lossy(&slice[1..1 + l]).to_string());
        slice = &slice[1 + l..];
    }
    parts.join(".")
}

fn build_dns_response(query: &[u8], spoof_ip: Ipv4Addr) -> Option<Vec<u8>> {
    let mut pos = 12;
    while pos < query.len() && query[pos] != 0 {
        pos += (query[pos] as usize) + 1;
    }
    pos += 5;
    if pos > query.len() {
        return None;
    }

    let mut resp = Vec::with_capacity(pos + 16);
    resp.extend_from_slice(&query[0..2]);
    resp.extend_from_slice(&[0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]);
    resp.extend_from_slice(&query[12..pos]);
    resp.extend_from_slice(&[0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x04]);
    resp.extend_from_slice(&spoof_ip.octets());
    Some(resp)
}

fn build_nxdomain_response(query: &[u8]) -> Option<Vec<u8>> {
    let mut pos = 12;
    while pos < query.len() && query[pos] != 0 {
        pos += (query[pos] as usize) + 1;
    }
    pos += 5;
    if pos > query.len() {
        return None;
    }

    let mut resp = Vec::with_capacity(pos);
    resp.extend_from_slice(&query[0..2]);
    resp.extend_from_slice(&[0x81, 0x83, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    resp.extend_from_slice(&query[12..pos]);
    Some(resp)
}
