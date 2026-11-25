use crate::assets;
use crate::logger::AppState;
use anyhow::Result;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout};

pub const TVOS_PKG: &str = "com.techgenius.tvoslauncher";
pub const TVOS_HOME_COMPONENT: &str = "com.techgenius.tvoslauncher/.MainActivity";

const PROTECTED_SYSTEM_PREFIXES: &[&str] = &[
    "android",
    "com.android.",
    "com.google.",
    "com.amlogic.",
    "com.realtek.",
    "com.broadcom.",
    "com.mediatek.",
    "com.qualcomm.",
    "org.codeaurora.",
];

const PROTECTED_EXACT_PACKAGES: &[&str] = &[
    "android",
    "com.shevery",
    "com.techgenius.tvoslauncher",
    "com.jio.remote.btserver",
    "com.media.jio.sso.stb",
    "com.jio.RequirementService",
];

const STOCK_LAUNCHERS: &[&str] = &[
    "com.jio.stb.screens",
    "com.iwedia.ui.rjio.launcher.reliance",
    "com.jio.home",
    "com.rjil.jio.stb.home",
    "ai.rusttv.launcher",
];

const BOOT_AD_PACKAGES: &[&str] = &[
    "com.jio.stbadservice",
    "com.jio.adc.aka",
    "com.jio.adc.embedded",
    "com.jio.bioscopecontentservice",
    "com.jio.screensaver",
    "com.jio.stb.screensaver",
    "com.jio.media.stb.screensaver",
    "com.jio.systemstats",
    "com.rjil.podlogservice",
    "com.jio.stb.analytics",
];

const REDOWNLOAD_DAEMONS: &[&str] = &[
    "com.jio.httpupdate",
    "com.jio.usbupdate",
    "com.jio.fota",
    "com.jio.media.jstb.fota",
    "com.jio.stb.appstore",
    "com.jio.stb.downloadmanager",
    "com.rjil.jio.stb.commandreceiver",
    "com.jio.backgroundscheduler",
    "com.jio.bioscopecontentservice",
    "com.iwedia.httpupdate",
    "com.iwedia.updateservice",
    "com.iwedia.pvodupdateservice",
    "com.iwedia.updatercuotaservice",
];

#[derive(Clone, Debug, Serialize)]
pub struct PackageEntry {
    pub name: String,
    pub category: String, // user | system | bloat
    pub status: String,   // active | unbound | disabled
    pub protected: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RawCmdResult {
    pub ok: bool,
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn adb_exec(adb_bin: &Path, args: &[&str]) -> (bool, String, String, i32) {
    let mut cmd = Command::new(adb_bin);
    cmd.args(args);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    match cmd.output() {
        Ok(out) => (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
            out.status.code().unwrap_or(-1),
        ),
        Err(e) => (false, String::new(), format!("Failed to invoke adb: {}", e), -1),
    }
}

pub fn adb_shell(adb_bin: &Path, target: &str, shell_args: &[&str]) -> String {
    let mut args = vec!["-s", target, "shell"];
    args.extend_from_slice(shell_args);
    let (_, stdout, stderr, _) = adb_exec(adb_bin, &args);
    format!("{}{}", stdout, stderr).trim().to_string()
}

pub fn auto_detect_connected_stb(adb_bin: &Path) -> Option<String> {
    let (_, out, _, _) = adb_exec(adb_bin, &["devices"]);
    for line in out.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "device" && parts[0].contains(':') {
            return Some(parts[0].to_string());
        }
    }
    None
}

pub fn connect_target(adb_bin: &Path, ip: &str) -> Result<String, String> {
    let clean = ip.trim();
    if clean.is_empty() {
        return Err("Enter a valid STB IP address (e.g. 192.168.31.181)".to_string());
    }
    let target = if clean.contains(':') {
        clean.to_string()
    } else {
        format!("{}:5555", clean)
    };
    let (_, out, err, _) = adb_exec(adb_bin, &["connect", &target]);
    let (_, state_out, _, _) = adb_exec(adb_bin, &["-s", &target, "get-state"]);
    if state_out.trim() == "device" {
        Ok(target)
    } else {
        Err(format!(
            "Could not authorize {} ({}{}). Check TV screen and approve 'Allow USB Debugging'.",
            target,
            out.trim(),
            err.trim()
        ))
    }
}

pub fn is_jhs_box(adb_bin: &Path, target: &str) -> bool {
    if !adb_shell(adb_bin, target, &["pm", "list", "packages", "com.iwedia"]).is_empty() {
        return true;
    }
    for prop in ["ro.product.model", "ro.build.product", "ro.product.device"] {
        let val = adb_shell(adb_bin, target, &["getprop", prop]).to_ascii_lowercase();
        if val.contains("jhs")
            || val.contains("bluebank")
            || val.contains("franklin")
            || val.contains("skyworth")
        {
            return true;
        }
    }
    false
}

pub fn is_protected_system_package(pkg: &str, is_jhs: bool) -> bool {
    let low = pkg.to_ascii_lowercase();
    if is_jhs && low == "com.jio.tr069client" {
        return true;
    }
    if PROTECTED_EXACT_PACKAGES.iter().any(|p| p.eq_ignore_ascii_case(pkg)) {
        return true;
    }
    if PROTECTED_SYSTEM_PREFIXES.iter().any(|prefix| low.starts_with(prefix)) {
        return true;
    }
    low.contains("btserver")
        || low.contains("bluetooth")
        || low.contains("inputmethod")
        || low.contains("webview")
        || low.contains("settings")
        || low.contains("sso")
}

pub fn is_known_bloat(pkg: &str) -> bool {
    STOCK_LAUNCHERS.contains(&pkg)
        || BOOT_AD_PACKAGES.contains(&pkg)
        || REDOWNLOAD_DAEMONS.contains(&pkg)
        || pkg == "com.jio.tr069client"
        || pkg == "insight.tr069.client"
}

pub fn neutralize_pkg(adb_bin: &Path, target: &str, pkg: &str) -> String {
    let _ = adb_shell(adb_bin, target, &["am", "force-stop", pkg]);
    let _ = adb_shell(adb_bin, target, &["pm", "clear", pkg]);
    let _ = adb_shell(adb_bin, target, &["pm", "suspend", "--user", "0", pkg]);
    let d_res = adb_shell(adb_bin, target, &["pm", "disable-user", "--user", "0", pkg]);
    let u_res = adb_shell(adb_bin, target, &["pm", "uninstall", "--user", "0", pkg]);
    if u_res.contains("Success") {
        "Unbound (user 0) & Locked".to_string()
    } else if d_res.contains("disabled") {
        "Disabled & Suspended".to_string()
    } else if u_res.is_empty() {
        "Not installed".to_string()
    } else {
        u_res
    }
}

pub fn list_packages(adb_bin: &Path, target: &str) -> Vec<PackageEntry> {
    let is_jhs = is_jhs_box(adb_bin, target);
    let user_raw = adb_shell(adb_bin, target, &["pm", "list", "packages", "-3"]);
    let all_u_raw = adb_shell(adb_bin, target, &["pm", "list", "packages", "-u"]);
    let active_raw = adb_shell(adb_bin, target, &["pm", "list", "packages"]);
    let disabled_raw = adb_shell(adb_bin, target, &["pm", "list", "packages", "-d"]);

    let user_set: HashSet<String> = user_raw
        .lines()
        .filter_map(|l| l.strip_prefix("package:"))
        .map(|s| s.trim().to_string())
        .collect();
    let active_set: HashSet<String> = active_raw
        .lines()
        .filter_map(|l| l.strip_prefix("package:"))
        .map(|s| s.trim().to_string())
        .collect();
    let disabled_set: HashSet<String> = disabled_raw
        .lines()
        .filter_map(|l| l.strip_prefix("package:"))
        .map(|s| s.trim().to_string())
        .collect();

    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for line in all_u_raw.lines() {
        if let Some(pkg) = line.strip_prefix("package:") {
            let name = pkg.trim().to_string();
            if name.is_empty() || !seen.insert(name.clone()) {
                continue;
            }
            let protected = is_protected_system_package(&name, is_jhs);
            let status = if !active_set.contains(&name) {
                "unbound"
            } else if disabled_set.contains(&name) {
                "disabled"
            } else {
                "active"
            };
            let category = if user_set.contains(&name) {
                "user"
            } else if is_known_bloat(&name) {
                "bloat"
            } else {
                "system"
            };

            // Hide core active Android system packages from cluttering the UI table,
            // but show them if they were accidentally unbound/disabled so user can restore them!
            if protected && status == "active" {
                continue;
            }

            out.push(PackageEntry {
                name,
                category: category.to_string(),
                status: status.to_string(),
                protected,
            });
        }
    }

    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn install_apk_file(adb_bin: &Path, target: &str, apk_path: &Path) -> Result<String, String> {
    if !apk_path.exists() {
        return Err(format!("APK file not found: {}", apk_path.display()));
    }
    let path_str = apk_path.to_string_lossy().to_string();
    let (ok, out, err, _) = adb_exec(adb_bin, &["-s", target, "install", "-r", "-d", "-g", &path_str]);
    if ok || out.contains("Success") {
        Ok(format!("Installed '{}' successfully!", apk_path.file_name().unwrap_or_default().to_string_lossy()))
    } else {
        Err(format!("{}{}", out.trim(), err.trim()))
    }
}

pub fn safe_uninstall(adb_bin: &Path, target: &str, pkgs: &[String]) -> Vec<String> {
    let is_jhs = is_jhs_box(adb_bin, target);
    let mut results = Vec::new();
    for pkg in pkgs {
        if is_protected_system_package(pkg, is_jhs) {
            results.push(format!("[BLOCKED] '{}' is a protected core OS/JHS package.", pkg));
        } else {
            let st = neutralize_pkg(adb_bin, target, pkg);
            results.push(format!("{}: {}", pkg, st));
        }
    }
    results
}

pub fn restore_packages(adb_bin: &Path, target: &str, pkgs: &[String]) -> Vec<String> {
    let mut results = Vec::new();
    for pkg in pkgs {
        let r1 = adb_shell(adb_bin, target, &["cmd", "package", "install-existing", pkg]);
        let _ = adb_shell(adb_bin, target, &["pm", "unsuspend", "--user", "0", pkg]);
        let r3 = adb_shell(adb_bin, target, &["pm", "enable", pkg]);
        results.push(format!("{}: {} | {}", pkg, r1, r3));
    }
    results
}

pub fn discover_home_launchers(adb_bin: &Path, target: &str) -> Vec<String> {
    let raw = adb_shell(
        adb_bin,
        target,
        &[
            "cmd", "package", "query-activities", "--brief",
            "-a", "android.intent.action.MAIN",
            "-c", "android.intent.category.HOME",
        ],
    );
    let mut comps: Vec<String> = raw
        .lines()
        .map(|l| l.trim())
        .filter(|l| l.contains('/') && !l.starts_with("priority") && !l.starts_with("preferred"))
        .map(|l| l.split_whitespace().last().unwrap_or(l).to_string())
        .filter(|c| {
            let low = c.to_ascii_lowercase();
            !low.starts_with("com.jio.stb.screens/")
                && !low.starts_with("com.iwedia.ui.rjio.launcher.reliance/")
                && !low.contains("bluebank")
                && !low.contains("com.android.settings")
                && !low.contains("com.android.tv.settings")
        })
        .collect();
    comps.sort();
    comps.dedup();
    comps
}

pub fn grant_tvos_permissions(adb_bin: &Path, target: &str) {
    let cmds: &[&[&str]] = &[
        &["pm", "grant", TVOS_PKG, "android.permission.WRITE_SECURE_SETTINGS"],
        &["pm", "grant", TVOS_PKG, "android.permission.PACKAGE_USAGE_STATS"],
        &["pm", "grant", TVOS_PKG, "android.permission.ACCESS_FINE_LOCATION"],
        &["pm", "grant", TVOS_PKG, "android.permission.ACCESS_COARSE_LOCATION"],
        &["appops", "set", TVOS_PKG, "GET_USAGE_STATS", "allow"],
        &["appops", "set", TVOS_PKG, "SYSTEM_ALERT_WINDOW", "allow"],
        &["appops", "set", TVOS_PKG, "WRITE_SETTINGS", "allow"],
        &["appops", "set", TVOS_PKG, "ACCESS_RESTRICTED_SETTINGS", "allow"],
        &["cmd", "notification", "allow_listener", "com.techgenius.tvoslauncher/com.techgenius.tvoslauncher.MediaWatchService"],
        &["dpm", "set-active-admin", "com.techgenius.tvoslauncher/.SleepReceiver$Admin"],
        &["settings", "put", "secure", "enabled_accessibility_services", "com.techgenius.tvoslauncher/com.techgenius.tvoslauncher.PowerService"],
        &["settings", "put", "secure", "accessibility_enabled", "1"],
        &["dumpsys", "deviceidle", "whitelist", "+com.techgenius.tvoslauncher"],
    ];
    for c in cmds {
        let _ = adb_shell(adb_bin, target, c);
    }
}

pub fn fix_home_button(adb_bin: &Path, target: &str) -> Vec<String> {
    let mut log = Vec::new();
    for pkg in STOCK_LAUNCHERS {
        let st = neutralize_pkg(adb_bin, target, pkg);
        log.push(format!("Stock launcher {}: {}", pkg, st));
    }
    let raw = adb_shell(
        adb_bin,
        target,
        &[
            "cmd", "package", "query-activities", "--brief",
            "-a", "android.intent.action.MAIN",
            "-c", "android.intent.category.HOME",
        ],
    );
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some((pkg, _)) = trimmed.split_once('/') {
            let pkg_clean = pkg.split_whitespace().last().unwrap_or(pkg);
            if pkg_clean.contains("jio") || pkg_clean.contains("iwedia") || pkg_clean.contains("bluebank") {
                let st = neutralize_pkg(adb_bin, target, pkg_clean);
                log.push(format!("Dynamic stock resolver {}: {}", pkg_clean, st));
            }
        }
    }
    if let Some(first) = discover_home_launchers(adb_bin, target).first() {
        let _ = adb_shell(adb_bin, target, &["cmd", "package", "set-home-activity", first]);
        log.push(format!("Locked default Home -> {}", first));
    }
    log
}

pub fn activate_launcher(adb_bin: &Path, target: &str, component: &str) -> Vec<String> {
    let mut log = Vec::new();
    let pkg = component.split('/').next().unwrap_or("");
    if !pkg.is_empty() {
        let _ = adb_shell(adb_bin, target, &["pm", "unsuspend", "--user", "0", pkg]);
        let _ = adb_shell(adb_bin, target, &["pm", "enable", pkg]);
        let _ = adb_shell(adb_bin, target, &["pm", "grant", pkg, "android.permission.WRITE_SECURE_SETTINGS"]);
        let _ = adb_shell(adb_bin, target, &["dumpsys", "deviceidle", "whitelist", &format!("+{}", pkg)]);
    }
    log.extend(fix_home_button(adb_bin, target));
    let res = adb_shell(adb_bin, target, &["cmd", "package", "set-home-activity", component]);
    let _ = adb_shell(adb_bin, target, &["am", "start", "-n", component]);
    log.push(format!("Default Home set to {} ({})", component, res));
    log
}

pub fn supercharge_tvos(adb_bin: &Path, target: &str, runtime_dir: &Path) -> Vec<String> {
    let mut log = Vec::new();
    let tvos_apk = runtime_dir.join("tvOS-Launcher.apk");
    match install_apk_file(adb_bin, target, &tvos_apk) {
        Ok(m) => log.push(m),
        Err(e) => log.push(format!("Install warning: {}", e)),
    }
    grant_tvos_permissions(adb_bin, target);
    log.push("Granted tvOS Launcher WRITE_SECURE_SETTINGS, Notification Listener, Admin & Accessibility".to_string());
    log.extend(activate_launcher(adb_bin, target, TVOS_HOME_COMPONENT));
    log
}

pub fn remove_boot_ads(adb_bin: &Path, target: &str) -> Vec<String> {
    let mut log = Vec::new();
    for pkg in BOOT_AD_PACKAGES {
        let st = neutralize_pkg(adb_bin, target, pkg);
        log.push(format!("{}: {}", pkg, st));
    }
    let _ = adb_shell(adb_bin, target, &["settings", "put", "secure", "screensaver_enabled", "0"]);
    let _ = adb_shell(adb_bin, target, &["settings", "put", "secure", "screensaver_activate_on_dock", "0"]);
    let _ = adb_shell(adb_bin, target, &["settings", "put", "secure", "screensaver_activate_on_sleep", "0"]);
    log.push("Disabled promo screensavers and boot ad injectors.".to_string());
    log
}

pub fn block_auto_redownloads(adb_bin: &Path, target: &str, keyword: Option<&str>) -> Vec<String> {
    let mut log = Vec::new();
    let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "ota_disable_automatic_update", "1"]);
    let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "package_verifier_enable", "0"]);

    let is_jhs = is_jhs_box(adb_bin, target);
    for pkg in REDOWNLOAD_DAEMONS {
        let st = neutralize_pkg(adb_bin, target, pkg);
        log.push(format!("{}: {}", pkg, st));
    }

    if !is_jhs {
        log.push(format!("com.jio.tr069client: {}", neutralize_pkg(adb_bin, target, "com.jio.tr069client")));
        log.push(format!("insight.tr069.client: {}", neutralize_pkg(adb_bin, target, "insight.tr069.client")));
    } else {
        log.push("[JHS PROTECTED] Keeping com.jio.tr069client enabled so port 5555 survives reboots.".to_string());
    }

    let mut keywords = vec!["etv".to_string(), "eenadu".to_string()];
    if let Some(kw) = keyword {
        let trimmed = kw.trim().to_ascii_lowercase();
        if !trimmed.is_empty() {
            keywords.push(trimmed);
        }
    }

    for kw in keywords {
        let matches = adb_shell(adb_bin, target, &["pm", "list", "packages", &kw]);
        for line in matches.lines() {
            if let Some(pkg) = line.strip_prefix("package:") {
                let pkg = pkg.trim();
                if !pkg.is_empty() && !is_protected_system_package(pkg, is_jhs) {
                    let st = neutralize_pkg(adb_bin, target, pkg);
                    log.push(format!("Blocked '{}' (match '{}'): {}", pkg, kw, st));
                }
            }
        }
    }
    log
}

pub fn boost_performance(adb_bin: &Path, target: &str, scale: &str) -> Vec<String> {
    let mut log = Vec::new();
    let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "window_animation_scale", scale]);
    let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "transition_animation_scale", scale]);
    let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "animator_duration_scale", scale]);
    let _ = adb_shell(adb_bin, target, &["pm", "trim-caches", "999999999999"]);
    let _ = adb_shell(adb_bin, target, &["am", "kill-all"]);
    log.push(format!("Animation scales set to {}x, package caches trimmed & idle tasks cleared.", scale));
    let mem = adb_shell(adb_bin, target, &["cat", "/proc/meminfo"]);
    for line in mem.lines().take(3) {
        log.push(line.to_string());
    }
    log
}

pub fn set_private_dns(adb_bin: &Path, target: &str, host: Option<&str>) -> String {
    match host {
        Some(h) if !h.trim().is_empty() => {
            let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "private_dns_mode", "hostname"]);
            let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "private_dns_specifier", h.trim()]);
            format!("Private DNS switched to '{}'", h.trim())
        }
        _ => {
            let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "private_dns_mode", "off"]);
            "Private DNS turned OFF (using router/DHCP DNS)".to_string()
        }
    }
}

pub fn backup_apk(adb_bin: &Path, target: &str, pkg: &str) -> Result<String, String> {
    let path_out = adb_shell(adb_bin, target, &["pm", "path", pkg]);
    let remote = path_out
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("package:"))
        .map(|s| s.trim())
        .ok_or_else(|| format!("Could not resolve APK path for '{}'", pkg))?;

    let _ = fs::create_dir_all("apk_backups");
    let dest = format!("apk_backups/{}.apk", pkg);
    let (ok, out, err, _) = adb_exec(adb_bin, &["-s", target, "pull", remote, &dest]);
    if ok {
        Ok(format!("Backed up {} -> {}", pkg, dest))
    } else {
        Err(format!("{}{}", out.trim(), err.trim()))
    }
}

pub fn remote_action(adb_bin: &Path, target: &str, action: &str, text: Option<&str>) -> Result<String, String> {
    match action {
        "settings" => {
            let _ = adb_shell(adb_bin, target, &["am", "start", "-a", "android.settings.SETTINGS"]);
            Ok("Opened Stock Android Settings on TV".to_string())
        }
        "dev_options" => {
            let _ = adb_shell(adb_bin, target, &["am", "start", "-a", "android.settings.APPLICATION_DEVELOPMENT_SETTINGS"]);
            Ok("Opened Developer Options on TV".to_string())
        }
        "home" => {
            let _ = adb_shell(adb_bin, target, &["input", "keyevent", "KEYCODE_HOME"]);
            Ok("Sent HOME button press to TV".to_string())
        }
        "back" => {
            let _ = adb_shell(adb_bin, target, &["input", "keyevent", "KEYCODE_BACK"]);
            Ok("Sent BACK button press to TV".to_string())
        }
        "text" => {
            let txt = text.unwrap_or("").trim().replace(' ', "%s");
            if txt.is_empty() {
                return Err("Text cannot be empty".to_string());
            }
            let _ = adb_shell(adb_bin, target, &["input", "text", &txt]);
            Ok("Sent text to TV input field".to_string())
        }
        _ => Err("Unknown remote action".to_string()),
    }
}

pub fn capture_screenshot_bytes(adb_bin: &Path, target: &str) -> Result<Vec<u8>, String> {
    let runtime = assets::extract_adb_only().map_err(|e| e.to_string())?;
    let local_png = runtime.join("stb_screenshot.png");
    let _ = adb_shell(adb_bin, target, &["screencap", "-p", "/sdcard/stb_screen.png"]);
    let (ok, out, err, _) = adb_exec(
        adb_bin,
        &["-s", target, "pull", "/sdcard/stb_screen.png", local_png.to_str().unwrap()],
    );
    let _ = adb_shell(adb_bin, target, &["rm", "/sdcard/stb_screen.png"]);
    if !ok {
        return Err(format!("Screenshot pull failed: {}{}", out.trim(), err.trim()));
    }
    fs::read(&local_png).map_err(|e| e.to_string())
}

pub fn reboot_action(adb_bin: &Path, target: &str, mode: &str) -> Result<String, String> {
    match mode {
        "normal" => {
            let _ = adb_exec(adb_bin, &["-s", target, "reboot"]);
            Ok("Reboot command sent to STB".to_string())
        }
        "tcpip" => {
            let _ = adb_exec(adb_bin, &["-s", target, "tcpip", "5555"]);
            Ok("Restarted Wireless ADB daemon on port 5555".to_string())
        }
        "recovery" => {
            let _ = adb_exec(adb_bin, &["-s", target, "reboot", "recovery"]);
            Ok("Rebooting STB into Recovery Mode".to_string())
        }
        "bootloader" => {
            let _ = adb_exec(adb_bin, &["-s", target, "reboot", "bootloader"]);
            Ok("Rebooting STB into Bootloader / Fastboot".to_string())
        }
        _ => Err("Invalid reboot mode".to_string()),
    }
}

fn split_shell_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    for ch in input.chars() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            c if c.is_whitespace() && !in_single && !in_double => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    args
}

pub fn run_raw_adb_command(adb_bin: &Path, target: Option<&str>, raw_cmd: &str) -> RawCmdResult {
    let mut tokens = split_shell_args(raw_cmd.trim());
    if tokens.is_empty() {
        return RawCmdResult {
            ok: false,
            command: String::new(),
            stdout: String::new(),
            stderr: "Enter an ADB command (e.g. 'shell getprop ro.product.model' or 'devices')".to_string(),
            exit_code: 1,
        };
    }

    if tokens[0].eq_ignore_ascii_case("adb") || tokens[0].eq_ignore_ascii_case("adb.exe") {
        tokens.remove(0);
    }
    if tokens.is_empty() {
        return RawCmdResult {
            ok: false,
            command: "adb".to_string(),
            stdout: String::new(),
            stderr: "Missing subcommand after 'adb'".to_string(),
            exit_code: 1,
        };
    }

    let first = tokens[0].to_ascii_lowercase();
    let is_global = matches!(
        first.as_str(),
        "devices" | "connect" | "disconnect" | "kill-server" | "start-server" | "version" | "help" | "-s"
    );

    let mut final_args: Vec<String> = Vec::new();
    if !is_global {
        if let Some(t) = target {
            final_args.push("-s".to_string());
            final_args.push(t.to_string());
        }
    }
    final_args.extend(tokens);

    let arg_refs: Vec<&str> = final_args.iter().map(|s| s.as_str()).collect();
    let (ok, stdout, stderr, exit_code) = adb_exec(adb_bin, &arg_refs);
    RawCmdResult {
        ok,
        command: format!("adb {}", final_args.join(" ")),
        stdout,
        stderr,
        exit_code,
    }
}

pub async fn provision_stb(stb_ip: IpAddr, app: Arc<AppState>) -> Result<String> {
    let runtime_dir = assets::extract_runtime_assets()?;
    let adb_bin = assets::adb_bin_path(&runtime_dir);
    let tvos_apk = runtime_dir.join("tvOS-Launcher.apk");
    let shevery_apk = runtime_dir.join("shevery-release.apk");

    let target = format!("{}:5555", stb_ip);
    let sock_addr: SocketAddr = target.parse()?;
    app.set_step(4, "Waiting for Wireless ADB (:5555)");
    app.log("ADB", "info", format!("Checking if TCP port 5555 is open on {}...", stb_ip));

    let mut port_open = false;
    for i in 0..60 {
        if app.is_cancelled() {
            anyhow::bail!("Cancelled by user");
        }
        if timeout(Duration::from_millis(1200), TcpStream::connect(&sock_addr))
            .await
            .map(|r| r.is_ok())
            .unwrap_or(false)
        {
            port_open = true;
            if let Ok(mut st) = app.inner.lock() {
                st.port5555_open = true;
            }
            app.log("ADB", "ok", format!("Port 5555 is OPEN on {}!", stb_ip));
            break;
        }
        if i == 5 {
            app.log(
                "ADB",
                "warn",
                "Still waiting for port 5555 — if CWMP succeeded, cold-reboot STB or toggle USB Debugging ON.",
            );
        }
        sleep(Duration::from_secs(2)).await;
    }

    if !port_open {
        let (cwmp_ok, tls_rej) = app
            .inner
            .lock()
            .map(|st| (st.cwmp_set_ok, st.tls_rejection.clone()))
            .unwrap_or((false, None));

        if let Some(rej) = tls_rej {
            app.set_failure(
                "CWMP / TLS",
                "STB Rejected TLS Certificate (Port 5555 Never Opened)",
                rej,
                vec![
                    "JHSB200 / JHSC200 / JHSD200 require JHS mode with eMudhra forged_chain.pem.",
                    "JMSC200A / JMSB200A require JMS mode with RILROOTCA acs_chain.pem.",
                    "Verify the STB clock synced via NTP (:123) and cold-boot by pulling the 12V plug.",
                ],
            );
        } else if cwmp_ok {
            app.set_failure(
                "ADB :5555",
                "CWMP Succeeded, But Port 5555 Timed Out",
                format!("TR-069 accepted SetParameterValues, yet {}:5555 stayed closed after 120s.", stb_ip),
                vec![
                    "Pull the 12V DC power plug from the STB for 5 seconds and reconnect it.",
                    "Or open Developer Options on the STB and toggle USB Debugging OFF then ON.",
                ],
            );
        } else {
            app.set_failure(
                "ADB :5555",
                "Timed Out Waiting for Port 5555",
                format!("No TR-069 SetParameterValues success and {}:5555 remained closed.", stb_ip),
                vec![
                    "Ensure STB Static IP DNS 1 matches your Host LAN IP and DNS 2 is 0.0.0.0 / blank.",
                    "Unplug the router fiber cable if Jio's cloud ACS is racing your PC.",
                    "Pull the 12V power cord (do not use remote standby) and wait 60 seconds.",
                ],
            );
        }
        anyhow::bail!("Timed out waiting for port 5555");
    }

    app.log("ADB", "info", format!("Connecting to {} — click 'Allow USB Debugging' on your TV!", target));
    let mut connected = false;
    for _ in 0..35 {
        if app.is_cancelled() {
            anyhow::bail!("Cancelled by user");
        }
        let adb_c = adb_bin.clone();
        let tgt_c = target.clone();
        let is_dev = tokio::task::spawn_blocking(move || {
            let _ = adb_exec(&adb_c, &["connect", &tgt_c]);
            let (_, out, _, _) = adb_exec(&adb_c, &["-s", &tgt_c, "get-state"]);
            out.trim() == "device"
        })
        .await
        .unwrap_or(false);

        if is_dev {
            connected = true;
            if let Ok(mut st) = app.inner.lock() {
                st.adb_authorized = true;
            }
            app.log("ADB", "ok", format!("Authorized & connected to {}!", target));
            break;
        }
        sleep(Duration::from_secs(2)).await;
    }

    if !connected {
        app.set_failure(
            "ADB Auth",
            "USB Debugging Authorization Not Approved",
            format!("Connected to {}, but the TV prompt was not accepted within 70 seconds.", target),
            vec![
                "Check your TV screen for the 'Allow USB Debugging?' dialog.",
                "Tick 'Always allow from this computer' and press OK with the Jio remote.",
                "Then click Connect in the top bar or re-run Provisioning.",
            ],
        );
        anyhow::bail!("ADB authorization timed out");
    }

    app.set_step(5, "Sideloading Launchers & Hardening STB");
    let adb_run = adb_bin.clone();
    let tgt_run = target.clone();
    let app_run = app.clone();

    let summary = tokio::task::spawn_blocking(move || {
        let mut done_items = Vec::new();

        app_run.log("ADB", "info", "Persisting ADB and disabling OTA package verifiers...");
        let _ = adb_shell(&adb_run, &tgt_run, &["settings", "put", "global", "adb_enabled", "1"]);
        let _ = adb_shell(&adb_run, &tgt_run, &["settings", "put", "global", "ota_disable_automatic_update", "1"]);
        let _ = adb_shell(&adb_run, &tgt_run, &["settings", "put", "global", "package_verifier_enable", "0"]);
        done_items.push("Persistent Wireless ADB enabled & OTA verifier disabled".to_string());

        app_run.log("ADB", "info", "Sideloading & supercharging tvOS Launcher...");
        match install_apk_file(&adb_run, &tgt_run, &tvos_apk) {
            Ok(m) => app_run.log("ADB", "ok", m),
            Err(e) => app_run.log("ADB", "warn", format!("tvOS Launcher install warning: {}", e)),
        }
        grant_tvos_permissions(&adb_run, &tgt_run);
        done_items.push("tvOS Launcher installed with full privileged permissions".to_string());

        app_run.log("ADB", "info", "Sideloading & granting privileged access to Shevery Manager...");
        match install_apk_file(&adb_run, &tgt_run, &shevery_apk) {
            Ok(m) => app_run.log("ADB", "ok", m),
            Err(e) => app_run.log("ADB", "warn", format!("Shevery install warning: {}", e)),
        }
        let shevery_cmds: &[&[&str]] = &[
            &["pm", "grant", "com.shevery", "android.permission.WRITE_SECURE_SETTINGS"],
            &["pm", "grant", "com.shevery", "android.permission.DUMP"],
            &["pm", "grant", "com.shevery", "android.permission.PACKAGE_USAGE_STATS"],
            &["appops", "set", "com.shevery", "GET_USAGE_STATS", "allow"],
            &["appops", "set", "com.shevery", "WRITE_SETTINGS", "allow"],
            &["appops", "set", "com.shevery", "RUN_IN_BACKGROUND", "allow"],
            &["appops", "set", "com.shevery", "RUN_ANY_IN_BACKGROUND", "allow"],
            &["dumpsys", "deviceidle", "whitelist", "+com.shevery"],
        ];
        for c in shevery_cmds {
            let _ = adb_shell(&adb_run, &tgt_run, c);
        }
        done_items.push("Shevery Manager installed & battery-whitelisted".to_string());

        app_run.log("ADB", "info", "Neutralizing stock Jio / Iwedia launchers & locking Home button...");
        let _ = activate_launcher(&adb_run, &tgt_run, TVOS_HOME_COMPONENT);
        done_items.push("Stock Jio Launchers disabled — Home button locked to tvOS Launcher".to_string());

        app_run.log("ADB", "info", "Removing boot ads, ad injectors & promo screensavers...");
        let _ = remove_boot_ads(&adb_run, &tgt_run);
        done_items.push("Startup boot ads & screensaver promos neutralized".to_string());

        app_run.log("ADB", "info", "Blocking silent app auto-redownloads (ETV, FOTA & Iwedia)...");
        let is_jhs = is_jhs_box(&adb_run, &tgt_run);
        let _ = block_auto_redownloads(&adb_run, &tgt_run, None);
        if is_jhs {
            app_run.log("ADB", "ok", "JHS box detected: preserved com.jio.tr069client so port 5555 survives reboots.");
            done_items.push("JHS Auto-Redownload & Iwedia OTA shield active (tr069client preserved for ADB)".to_string());
        } else {
            done_items.push("JMS FOTA, tr069client & silent app push daemons blocked".to_string());
        }

        done_items
    })
    .await?;

    app.log("SUCCESS", "ok", format!("FerroSTB v1.0.0 provisioning complete for {}!", target));
    app.set_success(target.clone(), summary);
    Ok(target)
}
