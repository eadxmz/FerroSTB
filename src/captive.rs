use crate::logger::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub async fn run_captive_portal(listener: TcpListener, app: Arc<AppState>) {
    let logged = Arc::new(AtomicBool::new(false));
    loop {
        if let Ok((mut stream, peer)) = listener.accept().await {
            let app_c = app.clone();
            let log_flag = logged.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let resp = "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(resp.as_bytes()).await;
                if !log_flag.swap(true, Ordering::Relaxed) {
                    if let Ok(mut st) = app_c.inner.lock() {
                        st.captive_passed = true;
                    }
                    app_c.log(
                        "HTTP",
                        "ok",
                        format!("Served 204 No Content to {} (Captive Portal bypassed)", peer.ip()),
                    );
                }
            });
        }
    }
}
