use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;

const NTP_EPOCH_DELTA: u64 = 2_208_988_800;
static LOGGED_NTP: AtomicBool = AtomicBool::new(false);

pub async fn run_ntp_server(socket: UdpSocket) {
    let mut buf = [0u8; 48];
    loop {
        let (len, peer) = match socket.recv_from(&mut buf).await {
            Ok(res) => res,
            Err(_) => continue,
        };
        if len < 48 {
            continue;
        }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let secs = (now.as_secs() + NTP_EPOCH_DELTA) as u32;
        let frac = ((now.subsec_nanos() as u64 * (1u64 << 32)) / 1_000_000_000) as u32;

        let mut resp = [0u8; 48];
        let version = buf[0] & 0x38;
        resp[0] = if version == 0 { 0x24 } else { version | 0x04 };
        resp[1] = 2;
        resp[2] = 6;
        resp[3] = 0xEC;
        resp[12..16].copy_from_slice(b"LOCL");

        resp[16..20].copy_from_slice(&secs.to_be_bytes());
        resp[20..24].copy_from_slice(&frac.to_be_bytes());
        resp[24..32].copy_from_slice(&buf[40..48]);
        resp[32..36].copy_from_slice(&secs.to_be_bytes());
        resp[36..40].copy_from_slice(&frac.to_be_bytes());
        resp[40..44].copy_from_slice(&secs.to_be_bytes());
        resp[44..48].copy_from_slice(&frac.to_be_bytes());

        let _ = socket.send_to(&resp, peer).await;
        if !LOGGED_NTP.swap(true, Ordering::Relaxed) {
            println!("[NTP:123] Synced STB clock for {} to current 2026 UTC time", peer.ip());
        }
    }
}
