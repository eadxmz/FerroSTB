mod adb;
mod assets;
mod captive;
mod cwmp;
mod dns;
mod ntp;

use anyhow::Result;
use local_ip_address::local_ip;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::io::{self, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::process::Command;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::watch;
use tokio::time::{sleep, timeout};

#[cfg(target_os = "windows")]
const FW_RULE_NAME: &str = "FerroSTB_AutoAllow";

#[tokio::main]
async fn main() -> Result<()> {
    println!("=========================================================");
    println!("  FerroSTB v0.7.0 - High-Speed Rust STB Provisioner      ");
    println!("  Bundled with tvOS Launcher, Shevery & Ad-Free Shield   ");
    println!("=========================================================");

    let host_ip = match local_ip()? {
        IpAddr::V4(ip) => ip,
        _ => anyhow::bail!("Could not detect a valid LAN IPv4 address"),
    };

    println!("[+] Host LAN IPv4 Address : {}", host_ip);
    println!("[!] Action Required on STB: Set Static IP DNS 1 = {}", host_ip);
    println!("---------------------------------------------------------");

    ensure_firewall_allow_rules();
    stop_host_time_daemon();

    let (stb_ip_tx, mut stb_ip_rx) = watch::channel::<Option<IpAddr>>(None);
    let (cwmp_done_tx, mut cwmp_done_rx) = watch::channel::<Option<IpAddr>>(None);
    let (port5555_tx, mut port5555_rx) = watch::channel::<Option<IpAddr>>(None);

    let dns_sock = acquire_dns_socket(host_ip);

    let ntp_sock = match UdpSocket::bind("0.0.0.0:123").await {
        Ok(s) => {
            println!("[NTP] Time-Sync Daemon listening on 0.0.0.0:123");
            Some(s)
        }
        Err(e) => {
            eprintln!("[!] NTP Server (:123) warning: {}", e);
            None
        }
    };
    let ntp_active = ntp_sock.is_some();

    let http_listener = match TcpListener::bind("0.0.0.0:80").await {
        Ok(l) => {
            println!("[HTTP] Captive Portal 204 Spoofer listening on 0.0.0.0:80");
            Some(l)
        }
        Err(e) => {
            eprintln!("[!] Captive Portal (:80) warning: {}", e);
            None
        }
    };

    if let Some(sock) = dns_sock {
        let tx = stb_ip_tx.clone();
        tokio::spawn(async move {
            dns::run_dns_server(sock, host_ip, ntp_active, tx).await;
        });
    }
    if let Some(sock) = ntp_sock {
        tokio::spawn(async move {
            ntp::run_ntp_server(sock).await;
        });
    }
    if let Some(listener) = http_listener {
        tokio::spawn(async move {
            captive::run_captive_portal(listener).await;
        });
    }

    cwmp::run_cwmp_servers(stb_ip_tx.clone(), cwmp_done_tx).await?;

    println!("=========================================================");
    println!("[READY] ALL SERVERS LISTENING!");
    println!(">>> Unplug the Jio STB power cord, wait 5s, and plug in.");
    println!("=========================================================\n");

    let mut watcher_spawned = false;

    let target_stb_ip = loop {
        tokio::select! {
            Ok(()) = stb_ip_rx.changed() => {
                if let Some(ip) = *stb_ip_rx.borrow() {
                    if !watcher_spawned {
                        watcher_spawned = true;
                        println!("[+] STB detected on LAN at {}! Starting parallel port 5555 detector...", ip);
                        let p_tx = port5555_tx.clone();
                        tokio::spawn(async move {
                            let addr = SocketAddr::new(ip, 5555);
                            loop {
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
                        });
                    }
                }
            }
            Ok(()) = port5555_rx.changed() => {
                if let Some(stb_ip) = *port5555_rx.borrow() {
                    println!("\n[+] Port 5555 detected OPEN on {}! Launching ADB sideloader immediately...", stb_ip);
                    break stb_ip;
                }
            }
            Ok(()) = cwmp_done_rx.changed() => {
                if let Some(stb_ip) = *cwmp_done_rx.borrow() {
                    println!("\n[+] CWMP sequence completed for {}! Starting ADB sideloader...", stb_ip);
                    break stb_ip;
                }
            }
        }
    };

    let res = adb::provision_stb(target_stb_ip).await;
    cleanup_firewall_and_services();

    if let Err(e) = res {
        eprintln!("\n[!] Provisioning error: {}", e);
    }

    print!("\nPress Enter to exit...");
    let _ = io::stdout().flush();
    let mut dummy = String::new();
    let _ = io::stdin().read_line(&mut dummy);

    Ok(())
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

fn ensure_firewall_allow_rules() {
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

        println!("[FW] Added temporary Windows Firewall allow rules for FerroSTB ports");
    }

    #[cfg(not(target_os = "windows"))]
    {
        for port in ["53/udp", "123/udp", "80/tcp", "443/tcp", "7443/tcp", "8443/tcp", "9443/tcp"] {
            let _ = Command::new("ufw").args(["allow", port]).output();
        }
        println!("[FW] Configured Linux firewall rules for FerroSTB ports");
    }
}

fn cleanup_firewall_and_services() {
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

fn acquire_dns_socket(host_ip: Ipv4Addr) -> Option<UdpSocket> {
    let wildcard = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 53);
    let lan_addr = SocketAddrV4::new(host_ip, 53);

    for addr in [wildcard, lan_addr] {
        if let Ok(std_sock) = try_bind_dns_std(addr) {
            if let Ok(tok_sock) = UdpSocket::from_std(std_sock) {
                println!("[DNS] Listening on {} -> Selective spoofing to {}", addr, host_ip);
                return Some(tok_sock);
            }
        }
    }

    println!("[*] Port 53 is occupied by a system service. Auto-freeing UDP :53...");

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
                        println!("[DNS] Auto-freed port 53! Listening on {} -> Selective spoofing to {}", addr, host_ip);
                        return Some(tok_sock);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(50));
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
                        println!("[DNS] Auto-freed port 53! Listening on {} -> Selective spoofing to {}", addr, host_ip);
                        return Some(tok_sock);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    eprintln!("[!] DNS Server (:53) failed even after auto-eviction (Run as Administrator / sudo!)");
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
                    println!("[*] Evicting PID {} holding UDP {}", pid, local_addr);
                    let _ = Command::new("taskkill").args(["/F", "/PID", pid]).output();
                }
            }
        }
    }
}
