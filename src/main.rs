mod adb;
mod assets;
mod captive;
mod cwmp;
mod dns;
mod logger;
mod ntp;
mod provisioner;

use anyhow::Result;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use cwmp::StbFamily;
use logger::AppState;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::net::TcpListener;

#[derive(Deserialize)]
struct StartProvisionReq {
    family: String,
}

#[derive(Deserialize)]
struct ConnectReq {
    ip: String,
}

#[derive(Deserialize)]
struct PackagesReq {
    packages: Vec<String>,
}

#[derive(Deserialize)]
struct PathReq {
    path: String,
}

#[derive(Deserialize)]
struct ComponentReq {
    component: String,
}

#[derive(Deserialize)]
struct KeywordReq {
    keyword: Option<String>,
}

#[derive(Deserialize)]
struct ScaleReq {
    scale: String,
}

#[derive(Deserialize)]
struct DnsReq {
    host: Option<String>,
}

#[derive(Deserialize)]
struct PackageSingleReq {
    package: String,
}

#[derive(Deserialize)]
struct RemoteReq {
    action: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct RebootReq {
    mode: String,
}

#[derive(Deserialize)]
struct RawCmdReq {
    command: String,
}

#[derive(Serialize)]
struct ApiMsg {
    ok: bool,
    message: String,
    lines: Vec<String>,
}

impl ApiMsg {
    fn ok(msg: impl Into<String>) -> Self {
        Self { ok: true, message: msg.into(), lines: Vec::new() }
    }
    fn ok_lines(msg: impl Into<String>, lines: Vec<String>) -> Self {
        Self { ok: true, message: msg.into(), lines }
    }
    fn err(msg: impl Into<String>) -> Self {
        Self { ok: false, message: msg.into(), lines: Vec::new() }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let runtime_dir = assets::extract_runtime_assets()?;
    let adb_bin = assets::adb_bin_path(&runtime_dir);
    let host_ip = provisioner::detect_host_ipv4()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "Unknown".to_string());

    let (jhs_ready, jhs_src) = match assets::load_jhs_forged_chain() {
        Some((_, src)) => (true, src),
        None => (false, "Missing forged_chain.pem".to_string()),
    };

    let auto_target = adb::auto_detect_connected_stb(&adb_bin);
    let state = Arc::new(AppState::new(host_ip.clone(), jhs_ready, jhs_src, auto_target));

    let app = Router::new()
        .route("/", get(serve_ui))
        .route("/api/state", get(api_state))
        .route("/api/provision/start", post(api_provision_start))
        .route("/api/provision/stop", post(api_provision_stop))
        .route("/api/adb/connect", post(api_adb_connect))
        .route("/api/adb/detect", post(api_adb_detect))
        .route("/api/adb/packages", get(api_adb_packages))
        .route("/api/adb/install-path", post(api_install_path))
        .route(
            "/api/adb/install-upload",
            post(api_install_upload).layer(DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        .route("/api/adb/uninstall", post(api_uninstall))
        .route("/api/adb/restore", post(api_restore))
        .route("/api/adb/backup", post(api_backup))
        .route("/api/adb/launchers", get(api_launchers))
        .route("/api/adb/set-launcher", post(api_set_launcher))
        .route("/api/adb/supercharge-tvos", post(api_supercharge_tvos))
        .route("/api/adb/fix-home", post(api_fix_home))
        .route("/api/adb/remove-ads", post(api_remove_ads))
        .route("/api/adb/block-redownloads", post(api_block_redownloads))
        .route("/api/adb/boost", post(api_boost))
        .route("/api/adb/private-dns", post(api_private_dns))
        .route("/api/adb/remote", post(api_remote))
        .route("/api/adb/screenshot", get(api_screenshot))
        .route("/api/adb/reboot", post(api_reboot))
        .route("/api/adb/raw", post(api_raw_cmd))
        .route("/api/app/shutdown", post(api_shutdown))
        .with_state(state.clone());

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let url = format!("http://127.0.0.1:{}", port);

    println!("=========================================================");
    println!("  FerroSTB v1.0.0 — Unified Jio STB Suite + Toolbox UI   ");
    println!("  Host LAN IP : {}", host_ip);
    println!("  Dashboard   : {}", url);
    println!("  Close this window or press Ctrl+C to exit cleanly.     ");
    println!("=========================================================");

    launch_app_window(&url);

    let shutdown_state = state.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            shutdown_state.stop_provision();
            provisioner::cleanup_firewall_and_services();
        })
        .await?;

    Ok(())
}

fn launch_app_window(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let profile = std::env::temp_dir().join("ferrostb_ui_profile");
        let _ = fs::create_dir_all(&profile);
        let profile_arg = format!("--user-data-dir={}", profile.to_string_lossy());
        let app_arg = format!("--app={}", url);

        let browsers = [
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        ];

        for bin in browsers {
            if PathBuf::from(bin).exists() {
                if Command::new(bin)
                    .args([&app_arg, "--window-size=1360,860", &profile_arg, "--no-first-run"])
                    .spawn()
                    .is_ok()
                {
                    return;
                }
            }
        }
        let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    }

    #[cfg(not(target_os = "windows"))]
    {
        let app_arg = format!("--app={}", url);
        let sudo_user = std::env::var("SUDO_USER").ok();
        let browsers = ["google-chrome", "chromium", "chromium-browser", "microsoft-edge", "brave-browser"];
        for bin in browsers {
            let ok = if let Some(ref u) = sudo_user {
                Command::new("su")
                    .args(["-", u, "-c", &format!("{} '{}' --window-size=1360,860 >/dev/null 2>&1 &", bin, app_arg)])
                    .spawn()
                    .is_ok()
            } else {
                Command::new(bin).args([&app_arg, "--window-size=1360,860"]).spawn().is_ok()
            };
            if ok {
                return;
            }
        }
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}

async fn serve_ui() -> Html<&'static str> {
    Html(assets::UI_INDEX_HTML)
}

async fn api_state(State(state): State<Arc<AppState>>) -> Json<logger::ProvisionSnapshot> {
    Json(state.snapshot())
}

async fn api_provision_start(
    State(state): State<Arc<AppState>>,
    Json(req): Json<StartProvisionReq>,
) -> Json<ApiMsg> {
    let fam = if req.family.eq_ignore_ascii_case("jms") {
        StbFamily::Jms
    } else {
        StbFamily::Jhs
    };
    let st = state.clone();
    tokio::spawn(async move {
        provisioner::run_provisioning_pipeline(fam, st).await;
    });
    Json(ApiMsg::ok("Provisioning started"))
}

async fn api_provision_stop(State(state): State<Arc<AppState>>) -> Json<ApiMsg> {
    state.stop_provision();
    provisioner::cleanup_firewall_and_services();
    Json(ApiMsg::ok("Provisioning stopped and firewall rules cleaned up"))
}

async fn api_adb_connect(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConnectReq>,
) -> Json<ApiMsg> {
    let ip = req.ip;
    let res = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().map_err(|e| e.to_string())?;
        let adb_bin = assets::adb_bin_path(&dir);
        adb::connect_target(&adb_bin, &ip)
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));

    match res {
        Ok(target) => {
            state.set_active_target(Some(target.clone()));
            Json(ApiMsg::ok(format!("Connected to {}", target)))
        }
        Err(e) => Json(ApiMsg::err(e)),
    }
}

async fn api_adb_detect(State(state): State<Arc<AppState>>) -> Json<ApiMsg> {
    let found = tokio::task::spawn_blocking(|| {
        let dir = assets::extract_adb_only().ok()?;
        let adb_bin = assets::adb_bin_path(&dir);
        adb::auto_detect_connected_stb(&adb_bin)
    })
    .await
    .unwrap_or(None);

    if let Some(t) = found {
        state.set_active_target(Some(t.clone()));
        Json(ApiMsg::ok(format!("Auto-detected {}", t)))
    } else {
        Json(ApiMsg::err("No authorized STB found in 'adb devices'"))
    }
}

fn require_target(state: &Arc<AppState>) -> Result<String, ApiMsg> {
    state
        .active_target()
        .ok_or_else(|| ApiMsg::err("No STB connected. Enter your STB IP in the top bar and click Connect."))
}

async fn api_adb_packages(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(serde_json::json!({ "ok": false, "message": e.message, "packages": [] })),
    };

    let pkgs = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().ok()?;
        let adb_bin = assets::adb_bin_path(&dir);
        Some(adb::list_packages(&adb_bin, &target))
    })
    .await
    .unwrap_or(None)
    .unwrap_or_default();

    Json(serde_json::json!({ "ok": true, "packages": pkgs }))
}

async fn api_install_path(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PathReq>,
) -> Json<ApiMsg> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(e),
    };
    let clean = req.path.trim().trim_matches('"').trim_matches('\'').to_string();
    let res = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().map_err(|e| e.to_string())?;
        let adb_bin = assets::adb_bin_path(&dir);
        adb::install_apk_file(&adb_bin, &target, &PathBuf::from(clean))
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));

    match res {
        Ok(m) => Json(ApiMsg::ok(m)),
        Err(e) => Json(ApiMsg::err(e)),
    }
}

async fn api_install_upload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Json<ApiMsg> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(e),
    };
    if body.is_empty() {
        return Json(ApiMsg::err("Uploaded APK file is empty"));
    }
    let raw_name = headers
        .get("x-filename")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("sideload.apk");
    let safe_name: String = raw_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .collect();
    let fname = if safe_name.is_empty() { "sideload.apk".to_string() } else { safe_name };

    let res = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().map_err(|e| e.to_string())?;
        let uploads = dir.join("uploads");
        fs::create_dir_all(&uploads).map_err(|e| e.to_string())?;
        let apk_path = uploads.join(&fname);
        fs::write(&apk_path, &body).map_err(|e| e.to_string())?;
        let adb_bin = assets::adb_bin_path(&dir);
        adb::install_apk_file(&adb_bin, &target, &apk_path)
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));

    match res {
        Ok(m) => Json(ApiMsg::ok(m)),
        Err(e) => Json(ApiMsg::err(e)),
    }
}

async fn api_uninstall(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PackagesReq>,
) -> Json<ApiMsg> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(e),
    };
    let lines = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().unwrap();
        let adb_bin = assets::adb_bin_path(&dir);
        adb::safe_uninstall(&adb_bin, &target, &req.packages)
    })
    .await
    .unwrap_or_default();
    Json(ApiMsg::ok_lines("Uninstall completed", lines))
}

async fn api_restore(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PackagesReq>,
) -> Json<ApiMsg> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(e),
    };
    let lines = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().unwrap();
        let adb_bin = assets::adb_bin_path(&dir);
        adb::restore_packages(&adb_bin, &target, &req.packages)
    })
    .await
    .unwrap_or_default();
    Json(ApiMsg::ok_lines("Packages restored", lines))
}

async fn api_backup(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PackageSingleReq>,
) -> Json<ApiMsg> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(e),
    };
    let res = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().map_err(|e| e.to_string())?;
        let adb_bin = assets::adb_bin_path(&dir);
        adb::backup_apk(&adb_bin, &target, &req.package)
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match res {
        Ok(m) => Json(ApiMsg::ok(m)),
        Err(e) => Json(ApiMsg::err(e)),
    }
}

async fn api_launchers(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(serde_json::json!({ "ok": false, "message": e.message, "launchers": [] })),
    };
    let list = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().unwrap();
        let adb_bin = assets::adb_bin_path(&dir);
        adb::discover_home_launchers(&adb_bin, &target)
    })
    .await
    .unwrap_or_default();
    Json(serde_json::json!({ "ok": true, "launchers": list }))
}

async fn api_set_launcher(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ComponentReq>,
) -> Json<ApiMsg> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(e),
    };
    let lines = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().unwrap();
        let adb_bin = assets::adb_bin_path(&dir);
        adb::activate_launcher(&adb_bin, &target, req.component.trim())
    })
    .await
    .unwrap_or_default();
    Json(ApiMsg::ok_lines("Launcher updated & locked", lines))
}

async fn api_supercharge_tvos(State(state): State<Arc<AppState>>) -> Json<ApiMsg> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(e),
    };
    let lines = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_runtime_assets().unwrap();
        let adb_bin = assets::adb_bin_path(&dir);
        adb::supercharge_tvos(&adb_bin, &target, &dir)
    })
    .await
    .unwrap_or_default();
    Json(ApiMsg::ok_lines("tvOS Launcher installed & supercharged", lines))
}

async fn api_fix_home(State(state): State<Arc<AppState>>) -> Json<ApiMsg> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(e),
    };
    let lines = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().unwrap();
        let adb_bin = assets::adb_bin_path(&dir);
        adb::fix_home_button(&adb_bin, &target)
    })
    .await
    .unwrap_or_default();
    Json(ApiMsg::ok_lines("Stock Jio launchers neutralized & Home button locked", lines))
}

async fn api_remove_ads(State(state): State<Arc<AppState>>) -> Json<ApiMsg> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(e),
    };
    let lines = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().unwrap();
        let adb_bin = assets::adb_bin_path(&dir);
        adb::remove_boot_ads(&adb_bin, &target)
    })
    .await
    .unwrap_or_default();
    Json(ApiMsg::ok_lines("Boot ads & promo screensavers removed", lines))
}

async fn api_block_redownloads(
    State(state): State<Arc<AppState>>,
    Json(req): Json<KeywordReq>,
) -> Json<ApiMsg> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(e),
    };
    let lines = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().unwrap();
        let adb_bin = assets::adb_bin_path(&dir);
        adb::block_auto_redownloads(&adb_bin, &target, req.keyword.as_deref())
    })
    .await
    .unwrap_or_default();
    Json(ApiMsg::ok_lines("Auto-redownload shield applied", lines))
}

async fn api_boost(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ScaleReq>,
) -> Json<ApiMsg> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(e),
    };
    let lines = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().unwrap();
        let adb_bin = assets::adb_bin_path(&dir);
        adb::boost_performance(&adb_bin, &target, &req.scale)
    })
    .await
    .unwrap_or_default();
    Json(ApiMsg::ok_lines("Performance & RAM boost complete", lines))
}

async fn api_private_dns(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DnsReq>,
) -> Json<ApiMsg> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(e),
    };
    let msg = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().unwrap();
        let adb_bin = assets::adb_bin_path(&dir);
        adb::set_private_dns(&adb_bin, &target, req.host.as_deref())
    })
    .await
    .unwrap_or_else(|e| e.to_string());
    Json(ApiMsg::ok(msg))
}

async fn api_remote(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RemoteReq>,
) -> Json<ApiMsg> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(e),
    };
    let res = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().map_err(|e| e.to_string())?;
        let adb_bin = assets::adb_bin_path(&dir);
        adb::remote_action(&adb_bin, &target, &req.action, req.text.as_deref())
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match res {
        Ok(m) => Json(ApiMsg::ok(m)),
        Err(e) => Json(ApiMsg::err(e)),
    }
}

async fn api_screenshot(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let target = match state.active_target() {
        Some(t) => t,
        None => return (StatusCode::BAD_REQUEST, "No STB connected").into_response(),
    };
    let res = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().map_err(|e| e.to_string())?;
        let adb_bin = assets::adb_bin_path(&dir);
        adb::capture_screenshot_bytes(&adb_bin, &target)
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));

    match res {
        Ok(bytes) => ([(header::CONTENT_TYPE, "image/png")], bytes).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

async fn api_reboot(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RebootReq>,
) -> Json<ApiMsg> {
    let target = match require_target(&state) {
        Ok(t) => t,
        Err(e) => return Json(e),
    };
    let res = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().map_err(|e| e.to_string())?;
        let adb_bin = assets::adb_bin_path(&dir);
        adb::reboot_action(&adb_bin, &target, &req.mode)
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match res {
        Ok(m) => Json(ApiMsg::ok(m)),
        Err(e) => Json(ApiMsg::err(e)),
    }
}

async fn api_raw_cmd(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RawCmdReq>,
) -> Json<adb::RawCmdResult> {
    let target = state.active_target();
    let res = tokio::task::spawn_blocking(move || {
        let dir = assets::extract_adb_only().unwrap();
        let adb_bin = assets::adb_bin_path(&dir);
        adb::run_raw_adb_command(&adb_bin, target.as_deref(), &req.command)
    })
    .await
    .unwrap();
    Json(res)
}

async fn api_shutdown(State(state): State<Arc<AppState>>) -> Json<ApiMsg> {
    state.stop_provision();
    provisioner::cleanup_firewall_and_services();
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        std::process::exit(0);
    });
    Json(ApiMsg::ok("Shutting down FerroSTB..."))
}
