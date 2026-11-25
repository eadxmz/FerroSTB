use crate::adb;
use crate::assets;
use crate::captive;
use crate::cwmp::{self, StbFamily};
use crate::dns;
use crate::logger::AppState;
use crate::ntp;
use anyhow::Result;
use local_ip_address::{list_afinet_netifas, local_ip};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::watch;
use tokio::time::{sleep, timeout};

#[cfg(target_os = "windows")]
const FW_RULE_NAME: &str = "FerroSTB_AutoAllow";

const IGNORED_IFACE_SUBSTRINGS: &[&str] = &[
    "warp", "cloudflare", "wireguard", "wg", "tailscale", "zerotier", "zt",
    "tun", "tap", "utun", "openvpn", "nord", "mullvad", "proton", "hamachi",
    "radmin", "vethernet", "wsl", "docker", "br-", "virbr", "vmnet", "vbox",
    "hyper-v", "loopback",
];

fn is_cgnat(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 100 && (64..=127).contains(&b)
}

fn is_rfc1918(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168)
}

fn score_lan_iface(name_lower: &str, ip: Ipv4Addr) -> i32 {
    let [a, b, c, _] = ip.octets();
    let mut score = 0;
    if a == 192 && b == 168 {
        score += 100;
        if c == 29 {
            score += 20; // JioFiber / JioAirFiber default LAN subnet
        }
    } else if a == 10 {
        score += 60;
    } else if a == 172 && (16..=31).contains(&b) {
        score += 10;
    }

    if name_lower.contains("ethernet")
        || name_lower.starts_with("eth")
        || name_lower.starts_with("enp")
        || name_lower.starts_with("en")
    {
        score += 30;
    } else if name_lower.contains("wi-fi")
        || name_lower.contains("wifi")
        || name_lower.starts_with("wlan")
        || name_lower.starts_with("wlp")
    {
        score += 25;
    }
    score
}

pub fn list_lan_ipv4s() -> Vec<(String, Ipv4Addr)> {
    let Ok(ifaces) = list_afinet_netifas() else {
        return Vec::new();
    };

    let mut scored: Vec<(i32, String, Ipv4Addr)> = ifaces
        .into_iter()
        .filter_map(|(name, addr)| {
            let IpAddr::V4(ip) = addr else { return None };
            if ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() || is_cgnat(ip) {
                return None;
            }
            if !is_rfc1918(ip) {
                return None;
            }
            // Exclude Cloudflare WARP's default virtual IPv4 on Windows
            if ip == Ipv4Addr::new(172, 16, 0, 2) {
                return None;
            }
            let lower = name.to_ascii_lowercase();
            if IGNORED_IFACE_SUBSTRINGS.iter().any(|bad| lower.contains(bad)) {
                return None;
            }
            Some((score_lan_iface(&lower, ip), name, ip))
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    let mut seen = std::collections::HashSet::new();
    scored
        .into_iter()
        .filter(|(_, _, ip)| seen.insert(*ip))
        .map(|(_, name, ip)| (name, ip))
        .collect()
}

pub fn detect_host_ipv4() -> Result<Ipv4Addr> {
    if let Some((_, ip)) = list_lan_ipv4s().into_iter().next() {
        return Ok(ip);
    }
    match local_ip()? {
        IpAddr::V4(ip) => Ok(ip),
        _ => anyhow::bail!("Could not detect a valid LAN IPv4 address"),
    }
}

pub async fn run_provisioning_pipeline(family: StbFamily, app: Arc<AppState>) {
    let family_str = match family {
        StbFamily::Jhs => "jhs",
        StbFamily::Jms => "jms",
    };
    app.reset_for_provision(family_str);

    let (cert_pem, cert_label) = match family {
        StbFamily::Jhs => match assets::load_jhs_forged_chain() {
            Some((pem, src)) => {
                app.log("INIT", "ok", format!("JHS mode selected — loaded eMudhra chain from {}", src));
                (pem, format!("JHS eMudhra — {}", src))
            }
            None => {
                app.set_failure(
                    "TLS Cert",
                    "Missing JHS eMudhra Certificate (forged_chain.pem)",
                    "JHSB200 / JHSC200 / JHSD200 reject the legacy RILROOTCA certificate and require forged_chain.pem.",
                    vec![
                        "Download forged_chain.pem and place it right next to ferrostb.exe (no rebuild needed).",
                        "Or place it at tls/forged_chain.pem in your repo and rebuild.",
                    ],
                );
                return;
            }
        },
        StbFamily::Jms => {
            app.log("INIT", "ok", "JMS mode selected — using embedded RILROOTCA (acs_chain.pem)");
            (assets::ACS_CHAIN_PEM.to_vec(), "JMS RILROOTCA".to_string())
        }
    };

    let host_ip = match app.host_ip().parse::<Ipv4Addr>().or_else(|_| detect_host_ipv4().map_err(|e| e.to_string())) {
        Ok(ip) => ip,
        Err(e) => {
            app.set_failure(
                "Network",
                "Could Not Detect Host LAN IPv4",
                e.to_string(),
                vec![
                    "Connect your PC to the same Wi-Fi or Ethernet router as the Jio STB.",
                    "Verify your adapter has an IPv4 address (e.g. 192.168.x.x).",
                ],
            );
            return;
        }
    };

    ensure_firewall_allow_rules(&app);
    stop_host_time_daemon();

    let (stb_ip_tx, mut stb_ip_rx) = watch::channel::<Option<IpAddr>>(None);
    let (cwmp_done_tx, mut cwmp_done_rx) = watch::channel::<Option<IpAddr>>(None);
    let (port5555_tx, mut port5555_rx) = watch::channel::<Option<IpAddr>>(None);

    let dns_sock = acquire_dns_socket(host_ip, &app);
    if dns_sock.is_none() {
        cleanup_firewall_and_services();
        app.set_failure(
            "DNS :53",
            "Failed to Bind UDP Port 53",
            "Another Windows/Linux service is holding UDP port 53 and could not be evicted.",
            vec![
                "Right-click ferrostb.exe and choose 'Run as Administrator' (or run 'sudo ./ferrostb' on Linux).",
                "Disable Windows Mobile Hotspot / Internet Connection Sharing (SharedAccess) or local DNS tools.",
            ],
        );
        return;
    }

    let ntp_sock = match UdpSocket::bind("0.0.0.0:123").await {
        Ok(s) => {
            if let Ok(mut st) = app.inner.lock() {
                st.ntp_bound = true;
            }
            app.log("NTP", "ok", "Time-Sync Daemon listening on UDP :123");
            Some(s)
        }
        Err(e) => {
            app.log("NTP", "warn", format!("Could not bind UDP :123 ({}) — continuing without local NTP", e));
            None
        }
    };
    let ntp_active = ntp_sock.is_some();

    let http_listener = match TcpListener::bind("0.0.0.0:80").await {
        Ok(l) => {
            if let Ok(mut st) = app.inner.lock() {
                st.http_bound = true;
            }
            app.log("HTTP", "ok", "Captive Portal 204 Spoofer listening on TCP :80");
            Some(l)
        }
        Err(e) => {
            app.log("HTTP", "warn", format!("Could not bind TCP :80 ({})", e));
            None
        }
    };

    if let Some(sock) = dns_sock {
        if let Ok(mut st) = app.inner.lock() {
            st.dns_bound = true;
        }
        let tx = stb_ip_tx.clone();
        let app_c = app.clone();
        app.register_task(tokio::spawn(async move {
            dns::run_dns_server(sock, host_ip, ntp_active, tx, app_c).await;
        }));
    }

    if let Some(sock) = ntp_sock {
        let app_c = app.clone();
        app.register_task(tokio::spawn(async move {
            ntp::run_ntp_server(sock, app_c).await;
        }));
    }

    if let Some(listener) = http_listener {
        let app_c = app.clone();
        app.register_task(tokio::spawn(async move {
            captive::run_captive_portal(listener, app_c).await;
        }));
    }

    match cwmp::run_cwmp_servers(family, cert_pem, &cert_label, stb_ip_tx.clone(), cwmp_done_tx, app.clone()).await {
        Ok(handles) => {
            if let Ok(mut st) = app.inner.lock() {
                st.cwmp_bound = true;
            }
            for h in handles {
                app.register_task(h);
            }
        }
        Err(e) => {
            app.abort_servers();
            cleanup_firewall_and_services();
            app.set_failure(
                "CWMP TLS",
                "Failed to Initialize CWMP TLS Servers",
                e.to_string(),
                vec![
                    "Ensure ports 443/7443/8443/9443 are not occupied by IIS, Apache, Nginx, or another FerroSTB instance.",
                    "Confirm your PEM certificate file contains both certificate chain and private key.",
                ],
            );
            return;
        }
    }

    app.set_step(1, "All Servers Live — Cold-Boot Your Jio STB");
    app.log(
        "READY",
        "ok",
        format!(
            "All servers live on {}! Set STB DNS 1 = {} (DNS 2 = 0.0.0.0) and pull the 12V DC power plug for 5s.",
            host_ip, host_ip
        ),
    );

    let mut watcher_spawned = false;
    let target_stb_ip = loop {
        if app.is_cancelled() {
            app.abort_servers();
            cleanup_firewall_and_services();
            return;
        }
        tokio::select! {
            _ = sleep(Duration::from_millis(500)) => {}
            Ok(()) = stb_ip_rx.changed() => {
                if let Some(ip) = *stb_ip_rx.borrow() {
                    if !watcher_spawned {
                        watcher_spawned = true;
                        app.log("INIT", "ok", format!("STB detected on LAN at {}! Starting parallel port 5555 detector...", ip));
                        let p_tx = port5555_tx.clone();
                        let app_w = app.clone();
                        app.register_task(tokio::spawn(async move {
                            let addr = SocketAddr::new(ip, 5555);
                            loop {
                                if app_w.is_cancelled() { break; }
                                if timeout(Duration::from_millis(900), TcpStream::connect(&addr))
                                    .await
                                    .map(|r| r.is_ok())
                                    .unwrap_or(false)
                                {
                                    let _ = p_tx.send(Some(ip));
                                    break;
                                }
                                sleep(Duration::from_millis(1500)).await;
                            }
                        }));
                    }
                }
            }
            Ok(()) = port5555_rx.changed() => {
                if let Some(stb_ip) = *port5555_rx.borrow() {
                    app.log("ADB", "ok", format!("Port 5555 detected OPEN on {}! Launching ADB sideloader...", stb_ip));
                    break stb_ip;
                }
            }
            Ok(()) = cwmp_done_rx.changed() => {
                if let Some(stb_ip) = *cwmp_done_rx.borrow() {
                    app.log("CWMP", "ok", format!("CWMP sequence finished for {}! Starting ADB sideloader...", stb_ip));
                    break stb_ip;
                }
            }
        }
    };

    let _ = adb::provision_stb(target_stb_ip, app.clone()).await;
    app.abort_servers();
    cleanup_firewall_and_services();
}

fn stop_host_time_daemon() {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("net").args(["stop", "w32time"]).output();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = Command::new("systemctl").args(["stop", "systemd-timesyncd"]).output();
    }
}

fn ensure_firewall_allow_rules(app: &Arc<AppState>) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("netsh")
            .args(["advfirewall", "firewall", "delete", "rule", &format!("name={}", FW_RULE_NAME)])
            .output();

        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(p) = exe_path.to_str() {
                let _ = Command::new("netsh")
                    .args([
                        "advfirewall", "firewall", "add", "rule",
                        &format!("name={}", FW_RULE_NAME),
                        "dir=in", "action=allow",
                        &format!("program={}", p),
                        "enable=yes", "profile=any",
                    ])
                    .output();
            }
        }

        let _ = Command::new("netsh")
            .args([
                "advfirewall", "firewall", "add", "rule",
                &format!("name={}", FW_RULE_NAME),
                "dir=in", "action=allow", "protocol=UDP", "localport=53,123", "profile=any",
            ])
            .output();

        let _ = Command::new("netsh")
            .args([
                "advfirewall", "firewall", "add", "rule",
                &format!("name={}", FW_RULE_NAME),
                "dir=in", "action=allow", "protocol=TCP", "localport=80,443,7443,8443,9443", "profile=any",
            ])
            .output();

        app.log("FW", "ok", "Added temporary Windows Firewall allow rules for FerroSTB ports");
    }

    #[cfg(not(target_os = "windows"))]
    {
        for port in ["53/udp", "123/udp", "80/tcp", "443/tcp", "7443/tcp", "8443/tcp", "9443/tcp"] {
            let _ = Command::new("ufw").args(["allow", port]).output();
        }
        app.log("FW", "ok", "Configured Linux firewall rules for FerroSTB ports");
    }
}

pub fn cleanup_firewall_and_services() {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("netsh")
            .args(["advfirewall", "firewall", "delete", "rule", &format!("name={}", FW_RULE_NAME)])
            .output();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = Command::new("systemctl").args(["start", "systemd-resolved"]).output();
        let _ = Command::new("systemctl").args(["start", "systemd-timesyncd"]).output();
    }
}

fn try_bind_dns_std(addr: SocketAddrV4) -> Result<std::net::UdpSocket> {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    let _ = sock.set_reuse_address(true);
    sock.set_nonblocking(true)?;
    sock.bind(&SockAddr::from(addr))?;
    Ok(sock.into())
}

fn acquire_dns_socket(host_ip: Ipv4Addr, app: &Arc<AppState>) -> Option<UdpSocket> {
    let wildcard = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 53);
    let lan_addr = SocketAddrV4::new(host_ip, 53);

    for addr in [wildcard, lan_addr] {
        if let Ok(std_sock) = try_bind_dns_std(addr) {
            if let Ok(tok_sock) = UdpSocket::from_std(std_sock) {
                app.log("DNS", "ok", format!("Listening on {} -> spoofing to {}", addr, host_ip));
                return Some(tok_sock);
            }
        }
    }

    app.log("DNS", "warn", "UDP :53 occupied — attempting automatic service eviction...");

    #[cfg(target_os = "windows")]
    {
        for svc in ["SharedAccess", "icssvc", "hns", "DnsService"] {
            let _ = Command::new("sc").args(["config", svc, "start=", "demand"]).output();
            let _ = Command::new("sc").args(["stop", svc]).output();
        }

        for _attempt in 0..6 {
            kill_udp_port_holder(53);
            for addr in [wildcard, lan_addr] {
                if let Ok(std_sock) = try_bind_dns_std(addr) {
                    if let Ok(tok_sock) = UdpSocket::from_std(std_sock) {
                        let _ = Command::new("net").args(["start", "Appinfo"]).spawn();
                        app.log("DNS", "ok", format!("Auto-freed port 53! Listening on {}", addr));
                        return Some(tok_sock);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(60));
        }
        let _ = Command::new("net").args(["start", "Appinfo"]).spawn();
    }

    #[cfg(not(target_os = "windows"))]
    {
        for svc in ["systemd-resolved", "dnsmasq", "named"] {
            let _ = Command::new("systemctl").args(["stop", svc]).output();
        }
        for _attempt in 0..4 {
            let _ = Command::new("fuser").args(["-k", "53/udp"]).output();
            for addr in [wildcard, lan_addr] {
                if let Ok(std_sock) = try_bind_dns_std(addr) {
                    if let Ok(tok_sock) = UdpSocket::from_std(std_sock) {
                        app.log("DNS", "ok", format!("Auto-freed port 53! Listening on {}", addr));
                        return Some(tok_sock);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(60));
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn kill_udp_port_holder(port: u16) {
    let my_pid = std::process::id().to_string();
    let target_suffix = format!(":{}", port);

    if let Ok(out) = Command::new("netstat").args(["-ano", "-p", "UDP"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 && parts[0].eq_ignore_ascii_case("UDP") {
                let local_addr = parts[1];
                let pid = parts[parts.len() - 1];
                if local_addr.ends_with(&target_suffix) && pid != "0" && pid != my_pid {
                    let _ = Command::new("taskkill").args(["/F", "/PID", pid]).output();
                }
            }
        }
    }
}
