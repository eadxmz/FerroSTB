use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

static LOGGED_CAPTIVE: AtomicBool = AtomicBool::new(false);

pub async fn run_captive_portal(listener: TcpListener) {
    loop {
        if let Ok((mut stream, peer)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let resp = "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                if !LOGGED_CAPTIVE.swap(true, Ordering::Relaxed) {
                    println!("[HTTP:80] Served 204 No Content to {} (Captive Portal check passed)", peer.ip());
                }
            });
        }
    }
}
