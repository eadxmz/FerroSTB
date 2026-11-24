use crate::assets;
use anyhow::Result;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout};

const TVOS_PKG: &str = "com.techgenius.tvoslauncher";
const TVOS_HOME_COMPONENT: &str = "com.techgenius.tvoslauncher/.MainActivity";

const STOCK_LAUNCHERS: &[&str] = &[
    "com.jio.stb.screens",
    "com.iwedia.ui.rjio.launcher.reliance",
    "com.jio.home",
    "com.rjil.jio.stb.home",
    "ai.rusttv.launcher",
];

const BOOT_AD_AND_PROMO_PACKAGES: &[&str] = &[
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

const AUTO_REDOWNLOAD_AND_OTA_DAEMONS: &[&str] = &[
    "com.jio.httpupdate",
    "com.jio.usbupdate",
    "com.rjil.jio.stb.commandreceiver",
    "com.jio.backgroundscheduler",
    "com.jio.media.jstb.fota",
    "com.jio.fota",
    "com.jio.stb.appstore",
    "com.jio.stb.downloadmanager",
    "com.iwedia.httpupdate",
    "com.iwedia.updateservice",
    "com.iwedia.pvodupdateservice",
    "com.iwedia.updatercuotaservice",
];

pub async fn provision_stb(stb_ip: IpAddr) -> Result<()> {
    let runtime_dir = assets::extract_runtime_assets()?;
    let adb_bin = assets::adb_bin_path(&runtime_dir);
    let tvos_apk = runtime_dir.join("tvOS-Launcher.apk");
    let shevery_apk = runtime_dir.join("shevery-release.apk");

    let target = format!("{}:5555", stb_ip);
    let sock_addr: SocketAddr = target.parse()?;
    println!("[ADB] Checking if TCP port 5555 is open on {}...", stb_ip);

    let mut port_open = false;
    for i in 0..60 {
        if timeout(Duration::from_millis(1200), TcpStream::connect(&sock_addr))
            .await
            .map(|r| r.is_ok())
            .unwrap_or(false)
        {
            println!("[ADB] Port 5555 is OPEN on {}!", stb_ip);
            port_open = true;
            break;
        }
        if i == 4 {
            println!("\n=========================================================");
            println!("[!] Waiting for port 5555 on {}...", stb_ip);
            println!(">>> Check the [DISCOVERY] & [SUCCESS] logs above.");
            println!(">>> If SetParameterValues succeeded, unplug/replug STB power");
            println!(">>> or toggle Developer Options -> USB Debugging ON.");
            println!("=========================================================\n");
        }
        sleep(Duration::from_secs(2)).await;
    }

    if !port_open {
        anyhow::bail!("Timed out waiting for port 5555 on {}. Review the [DISCOVERY] logs above.", stb_ip);
    }

    println!("[ADB] Connecting to {} (Check your TV and click 'Allow USB Debugging')...", target);
    let mut connected = false;
    for _ in 0..35 {
        let _ = Command::new(&adb_bin).args(["connect", &target]).output();
        let state = Command::new(&adb_bin).args(["-s", &target, "get-state"]).output()?;
        if String::from_utf8_lossy(&state.stdout).trim() == "device" {
            println!("[ADB] Authenticated and connected to {}!", target);
            connected = true;
            break;
        }
        sleep(Duration::from_secs(2)).await;
    }

    if !connected {
        anyhow::bail!("ADB connected to port 5555, but USB Debugging authorization was not approved on TV.");
    }

    // 1. Persist ADB & block automatic OTA / package verifiers
    run_adb(&adb_bin, &target, &["shell", "settings", "put", "global", "adb_enabled", "1"]);
    run_adb(&adb_bin, &target, &["shell", "settings", "put", "global", "ota_disable_automatic_update", "1"]);
    run_adb(&adb_bin, &target, &["shell", "settings", "put", "global", "package_verifier_enable", "0"]);

    // 2. Sideload & Supercharge tvOS Launcher (com.techgenius.tvoslauncher)
    println!("[ADB] Sideloading tvOS Launcher (tvOS-Launcher.apk)...");
    run_adb(&adb_bin, &target, &["install", "-r", "-d", "-g", tvos_apk.to_str().unwrap()]);
    grant_tvos_launcher_permissions(&adb_bin, &target);

    // 3. Sideload & harden Shevery Manager
    println!("[ADB] Sideloading Shevery Manager & granting full privileged access...");
    run_adb(&adb_bin, &target, &["install", "-r", "-d", "-g", shevery_apk.to_str().unwrap()]);
    run_adb(&adb_bin, &target, &["shell", "pm", "grant", "com.shevery", "android.permission.WRITE_SECURE_SETTINGS"]);
    run_adb(&adb_bin, &target, &["shell", "pm", "grant", "com.shevery", "android.permission.DUMP"]);
    run_adb(&adb_bin, &target, &["shell", "pm", "grant", "com.shevery", "android.permission.PACKAGE_USAGE_STATS"]);
    run_adb(&adb_bin, &target, &["shell", "appops", "set", "com.shevery", "GET_USAGE_STATS", "allow"]);
    run_adb(&adb_bin, &target, &["shell", "appops", "set", "com.shevery", "WRITE_SETTINGS", "allow"]);
    run_adb(&adb_bin, &target, &["shell", "appops", "set", "com.shevery", "RUN_IN_BACKGROUND", "allow"]);
    run_adb(&adb_bin, &target, &["shell", "appops", "set", "com.shevery", "RUN_ANY_IN_BACKGROUND", "allow"]);
    run_adb(&adb_bin, &target, &["shell", "dumpsys", "deviceidle", "whitelist", "+com.shevery"]);

    // 4. Disable & remove ALL stock Jio / Iwedia / Bluebank launchers so Home button never reverts
    println!("[ADB] Disabling & removing stock Jio Launchers (locking Home button)...");
    for pkg in STOCK_LAUNCHERS {
        neutralize_package(&adb_bin, &target, pkg);
    }
    disable_any_remaining_jio_home_resolvers(&adb_bin, &target);

    let home_comp = resolve_active_launcher_component(&adb_bin, &target);
    println!("[ADB] Locking Default Home Activity -> {}", home_comp);
    run_adb(&adb_bin, &target, &["shell", "cmd", "package", "set-home-activity", &home_comp]);

    // 5. Remove all boot ads, ad injectors, and screensaver promos
    println!("[ADB] Removing startup boot ads, ad injectors & promo screensavers...");
    for pkg in BOOT_AD_AND_PROMO_PACKAGES {
        neutralize_package(&adb_bin, &target, pkg);
    }
    run_adb(&adb_bin, &target, &["shell", "settings", "put", "secure", "screensaver_enabled", "0"]);
    run_adb(&adb_bin, &target, &["shell", "settings", "put", "secure", "screensaver_activate_on_dock", "0"]);
    run_adb(&adb_bin, &target, &["shell", "settings", "put", "secure", "screensaver_activate_on_sleep", "0"]);

    // 6. Block silent app auto-redownloads (ETV, OTA updaters, background push daemons)
    println!("[ADB] Blocking silent app auto-redownloads, FOTA & background updaters...");
    let is_jhs = is_jhs_box(&adb_bin, &target);
    for pkg in AUTO_REDOWNLOAD_AND_OTA_DAEMONS {
        neutralize_package(&adb_bin, &target, pkg);
    }
    if !is_jhs {
        neutralize_package(&adb_bin, &target, "com.jio.tr069client");
        neutralize_package(&adb_bin, &target, "insight.tr069.client");
    } else {
        println!("[ADB] JHS box detected (JHSB200/JHSC200/JHSD200): keeping com.jio.tr069client enabled so port 5555 persists across cold reboots!");
    }

    run_adb(&adb_bin, &target, &["shell", "am", "start", "-n", &home_comp]);
    println!("\n=========================================================");
    println!("[+] FerroSTB v0.8.0 + tvOS Launcher Provisioning Complete!");
    println!(">>> Open 'ferrostb-toolbox' anytime to sideload APKs,");
    println!(">>> restore system packages, or customize your Jio STB!");
    println!("=========================================================");
    Ok(())
}

fn is_jhs_box(adb_bin: &Path, target: &str) -> bool {
    if check_package_exists(adb_bin, target, "com.iwedia") {
        return true;
    }
    for prop in ["ro.product.model", "ro.build.product", "ro.product.device"] {
        if let Ok(out) = Command::new(adb_bin)
            .args(["-s", target, "shell", "getprop", prop])
            .output()
        {
            let val = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
            if val.contains("jhs")
                || val.contains("bluebank")
                || val.contains("franklin")
                || val.contains("skyworth")
            {
                return true;
            }
        }
    }
    false
}

fn grant_tvos_launcher_permissions(adb_bin: &Path, target: &str) {
    run_adb_quiet(adb_bin, target, &["shell", "pm", "grant", TVOS_PKG, "android.permission.WRITE_SECURE_SETTINGS"]);
    run_adb_quiet(adb_bin, target, &["shell", "pm", "grant", TVOS_PKG, "android.permission.PACKAGE_USAGE_STATS"]);
    run_adb_quiet(adb_bin, target, &["shell", "pm", "grant", TVOS_PKG, "android.permission.ACCESS_FINE_LOCATION"]);
    run_adb_quiet(adb_bin, target, &["shell", "pm", "grant", TVOS_PKG, "android.permission.ACCESS_COARSE_LOCATION"]);
    run_adb_quiet(adb_bin, target, &["shell", "appops", "set", TVOS_PKG, "GET_USAGE_STATS", "allow"]);
    run_adb_quiet(adb_bin, target, &["shell", "appops", "set", TVOS_PKG, "SYSTEM_ALERT_WINDOW", "allow"]);
    run_adb_quiet(adb_bin, target, &["shell", "appops", "set", TVOS_PKG, "WRITE_SETTINGS", "allow"]);
    run_adb_quiet(adb_bin, target, &["shell", "appops", "set", TVOS_PKG, "ACCESS_RESTRICTED_SETTINGS", "allow"]);
    run_adb_quiet(
        adb_bin,
        target,
        &["shell", "cmd", "notification", "allow_listener", "com.techgenius.tvoslauncher/com.techgenius.tvoslauncher.MediaWatchService"],
    );
    run_adb_quiet(
        adb_bin,
        target,
        &["shell", "dpm", "set-active-admin", "com.techgenius.tvoslauncher/.SleepReceiver$Admin"],
    );
    run_adb_quiet(
        adb_bin,
        target,
        &["shell", "settings", "put", "secure", "enabled_accessibility_services", "com.techgenius.tvoslauncher/com.techgenius.tvoslauncher.PowerService"],
    );
    run_adb_quiet(adb_bin, target, &["shell", "settings", "put", "secure", "accessibility_enabled", "1"]);
    run_adb_quiet(adb_bin, target, &["shell", "dumpsys", "deviceidle", "whitelist", "+com.techgenius.tvoslauncher"]);
}

fn resolve_active_launcher_component(adb_bin: &Path, target: &str) -> String {
    if check_package_exists(adb_bin, target, TVOS_PKG) {
        return TVOS_HOME_COMPONENT.to_string();
    }
    if let Ok(out) = Command::new(adb_bin)
        .args([
            "-s",
            target,
            "shell",
            "cmd",
            "package",
            "query-activities",
            "--brief",
            "-a",
            "android.intent.action.MAIN",
            "-c",
            "android.intent.category.HOME",
        ])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.contains('/') && !trimmed.starts_with("priority") && !trimmed.starts_with("preferred") {
                let comp = trimmed.split_whitespace().last().unwrap_or(trimmed);
                let low = comp.to_ascii_lowercase();
                if !low.contains("jio")
                    && !low.contains("iwedia")
                    && !low.contains("bluebank")
                    && !low.contains("settings")
                {
                    return comp.to_string();
                }
            }
        }
    }
    TVOS_HOME_COMPONENT.to_string()
}

fn neutralize_package(adb_bin: &Path, target: &str, pkg: &str) {
    run_adb_quiet(adb_bin, target, &["shell", "am", "force-stop", pkg]);
    run_adb_quiet(adb_bin, target, &["shell", "pm", "clear", pkg]);
    run_adb_quiet(adb_bin, target, &["shell", "pm", "suspend", "--user", "0", pkg]);
    run_adb_quiet(adb_bin, target, &["shell", "pm", "disable-user", "--user", "0", pkg]);
    run_adb_quiet(adb_bin, target, &["shell", "pm", "uninstall", "--user", "0", pkg]);
}

fn disable_any_remaining_jio_home_resolvers(adb_bin: &Path, target: &str) {
    if let Ok(out) = Command::new(adb_bin)
        .args([
            "-s",
            target,
            "shell",
            "cmd",
            "package",
            "query-activities",
            "--brief",
            "-a",
            "android.intent.action.MAIN",
            "-c",
            "android.intent.category.HOME",
        ])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some((pkg, _act)) = trimmed.split_once('/') {
                let pkg_clean = pkg.split_whitespace().last().unwrap_or(pkg);
                if pkg_clean.contains("jio")
                    || pkg_clean.contains("iwedia")
                    || pkg_clean.contains("bluebank")
                {
                    println!("  [adb] Neutralizing discovered stock Home resolver: {}", pkg_clean);
                    neutralize_package(adb_bin, target, pkg_clean);
                }
            }
        }
    }
}

fn check_package_exists(adb_bin: &Path, target: &str, needle: &str) -> bool {
    if let Ok(out) = Command::new(adb_bin)
        .args(["-s", target, "shell", "pm", "list", "packages", needle])
        .output()
    {
        let s = String::from_utf8_lossy(&out.stdout);
        return s.contains(needle);
    }
    false
}

fn run_adb(adb_bin: &Path, target: &str, args: &[&str]) {
    let mut full_args = vec!["-s", target];
    full_args.extend_from_slice(args);
    if let Ok(out) = Command::new(adb_bin).args(&full_args).output() {
        let s = String::from_utf8_lossy(&out.stdout);
        if !s.trim().is_empty() {
            println!("  [adb] {}", s.trim());
        }
    }
}

fn run_adb_quiet(adb_bin: &Path, target: &str, args: &[&str]) {
    let mut full_args = vec!["-s", target];
    full_args.extend_from_slice(args);
    let _ = Command::new(adb_bin).args(&full_args).output();
}
