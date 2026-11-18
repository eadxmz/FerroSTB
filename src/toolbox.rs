mod assets;

use anyhow::Result;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const TVOS_PKG: &str = "com.techgenius.tvoslauncher";
const TVOS_HOME_COMPONENT: &str = "com.techgenius.tvoslauncher/.MainActivity";

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
];

fn main() -> Result<()> {
    let runtime_dir = assets::extract_runtime_assets()?;
    let adb_bin = assets::adb_bin_path(&runtime_dir);

    println!("==================================================================");
    println!("  FerroSTB Toolbox v0.7.0 - Ultimate Jio STB Manager & Customizer ");
    println!("  Embedded ADB + tvOS Launcher | System Package Restore Included  ");
    println!("==================================================================");

    let mut target = auto_detect_connected_stb(&adb_bin);

    loop {
        let status_label = target
            .as_deref()
            .unwrap_or("NOT CONNECTED (Select Option 1 first)");

        println!("\n------------------------------------------------------------------");
        println!(" Active STB Target : {}", status_label);
        println!("------------------------------------------------------------------");
        println!("  1. Connect to Jio STB via Wireless ADB (<STB_IP>:5555)");
        println!("  2. Install / Sideload APK File(s) (.apk)");
        println!("  3. Uninstall Apps (Safe Mode: Protects Core OS & Remote)");
        println!("  4. Restore / Reinstall Uninstalled or Disabled System Packages");
        println!("  5. Change / Switch Default Launcher Whenever Wanted");
        println!("  6. Install / Supercharge Bundled tvOS Launcher (All Permissions)");
        println!("  7. Disable / Remove Stock Jio Launcher (Fix Home Button)");
        println!("  8. Remove Boot Ads, Ad Injectors & Promo Screensavers");
        println!("  9. Block App Auto-Redownloads (ETV, FOTA & Silent Push)");
        println!(" 10. Performance & RAM Booster (0.5x Animations + Trim Cache)");
        println!(" 11. DNS & System-Wide AdGuard Private DNS Switcher");
        println!(" 12. Backup / Extract Installed App (.apk) to Host PC");
        println!(" 13. Remote Helper: Open Android Settings / Type Text / Screenshot");
        println!(" 14. Power & Reboot Menu (Normal / Recovery / Bootloader)");
        println!("  0. Exit");
        print!("\nSelect an option (0-14): ");
        io::stdout().flush()?;

        let choice = read_line_trimmed();
        if choice == "0" {
            println!("Exiting FerroSTB Toolbox.");
            break;
        }

        if choice == "1" {
            if let Some(t) = connect_adb(&adb_bin) {
                target = Some(t);
            }
            continue;
        }

        let active_target = match &target {
            Some(t) => t.clone(),
            None => {
                println!("[!] No STB connected yet. Let's connect first:");
                match connect_adb(&adb_bin) {
                    Some(t) => {
                        target = Some(t.clone());
                        t
                    }
                    None => continue,
                }
            }
        };

        match choice.as_str() {
            "2" => sideload_apk(&adb_bin, &active_target),
            "3" => safe_uninstall_apps(&adb_bin, &active_target),
            "4" => restore_uninstalled_system_packages(&adb_bin, &active_target),
            "5" => change_launcher(&adb_bin, &active_target),
            "6" => install_and_supercharge_tvos(&adb_bin, &active_target, &runtime_dir),
            "7" => fix_home_button(&adb_bin, &active_target),
            "8" => remove_boot_ads(&adb_bin, &active_target),
            "9" => block_auto_redownloads(&adb_bin, &active_target),
            "10" => boost_performance_and_ram(&adb_bin, &active_target),
            "11" => configure_private_dns(&adb_bin, &active_target),
            "12" => backup_installed_apk(&adb_bin, &active_target),
            "13" => remote_helper_menu(&adb_bin, &active_target),
            "14" => reboot_menu(&adb_bin, &active_target),
            _ => println!("[!] Invalid option. Enter a number from 0 to 14."),
        }
    }

    Ok(())
}

fn read_line_trimmed() -> String {
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
    input.trim().to_string()
}

fn adb_exec(adb_bin: &Path, args: &[&str]) -> (bool, String, String) {
    match Command::new(adb_bin).args(args).output() {
        Ok(out) => (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        ),
        Err(e) => (false, String::new(), format!("Failed to run adb: {}", e)),
    }
}

fn adb_shell(adb_bin: &Path, target: &str, shell_args: &[&str]) -> String {
    let mut args = vec!["-s", target, "shell"];
    args.extend_from_slice(shell_args);
    let (_, stdout, stderr) = adb_exec(adb_bin, &args);
    format!("{}{}", stdout, stderr).trim().to_string()
}

fn neutralize_pkg(adb_bin: &Path, target: &str, pkg: &str) -> String {
    let _ = adb_shell(adb_bin, target, &["am", "force-stop", pkg]);
    let _ = adb_shell(adb_bin, target, &["pm", "clear", pkg]);
    let _ = adb_shell(adb_bin, target, &["pm", "suspend", "--user", "0", pkg]);
    let d_res = adb_shell(adb_bin, target, &["pm", "disable-user", "--user", "0", pkg]);
    let u_res = adb_shell(adb_bin, target, &["pm", "uninstall", "--user", "0", pkg]);
    if u_res.contains("Success") {
        "Unbound (user 0) & Locked".to_string()
    } else if d_res.contains("disabled") {
        "Disabled & Suspended".to_string()
    } else {
        u_res
    }
}

fn auto_detect_connected_stb(adb_bin: &Path) -> Option<String> {
    let (_, out, _) = adb_exec(adb_bin, &["devices"]);
    for line in out.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "device" && parts[0].contains(':') {
            println!("[+] Auto-detected connected STB: {}", parts[0]);
            return Some(parts[0].to_string());
        }
    }
    None
}

fn connect_adb(adb_bin: &Path) -> Option<String> {
    print!("Enter Jio STB IP Address (e.g. 192.168.31.181): ");
    let _ = io::stdout().flush();
    let ip = read_line_trimmed();
    if ip.is_empty() {
        println!("[!] IP address cannot be empty.");
        return None;
    }

    let target = if ip.contains(':') {
        ip
    } else {
        format!("{}:5555", ip)
    };

    println!("[*] Connecting to {}...", target);
    let (_, out, err) = adb_exec(adb_bin, &["connect", &target]);
    println!("    {}{}", out.trim(), err.trim());

    let (_, state_out, _) = adb_exec(adb_bin, &["-s", &target, "get-state"]);
    if state_out.trim() == "device" {
        println!("[+] Connected and authorized on {}!", target);
        Some(target)
    } else {
        println!("[!] STB is unauthorized or unreachable. Approve 'Allow USB Debugging' on TV and try again.");
        None
    }
}

fn sideload_apk(adb_bin: &Path, target: &str) {
    print!("Drag & drop an .apk file (or folder of .apks) here and press Enter: ");
    let _ = io::stdout().flush();
    let raw_path = read_line_trimmed();
    let clean_path = raw_path.trim_matches('"').trim_matches('\'').trim();

    let path_obj = PathBuf::from(clean_path);
    if clean_path.is_empty() || !path_obj.exists() {
        println!("[!] Path not found: '{}'", clean_path);
        return;
    }

    let mut apk_files = Vec::new();
    if path_obj.is_dir() {
        if let Ok(entries) = fs::read_dir(&path_obj) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("apk"))
                    .unwrap_or(false)
                {
                    apk_files.push(p);
                }
            }
        }
    } else {
        apk_files.push(path_obj);
    }

    if apk_files.is_empty() {
        println!("[!] No .apk files found at '{}'.", clean_path);
        return;
    }

    for apk in apk_files {
        let apk_str = apk.to_string_lossy();
        println!("[*] Installing '{}' onto {}...", apk_str, target);
        let (ok, out, err) = adb_exec(adb_bin, &["-s", target, "install", "-r", "-d", "-g", &apk_str]);
        if ok || out.contains("Success") {
            println!("[+] Installed '{}' successfully!", apk_str);
        } else {
            println!("[!] Install output:\n{}{}", out.trim(), err.trim());
        }
    }
}

fn is_protected_system_package(pkg: &str) -> bool {
    let low = pkg.to_ascii_lowercase();
    if PROTECTED_EXACT_PACKAGES.iter().any(|p| p.eq_ignore_ascii_case(pkg)) {
        return true;
    }
    if PROTECTED_SYSTEM_PREFIXES.iter().any(|prefix| low.starts_with(prefix)) {
        return true;
    }
    if low.contains("btserver")
        || low.contains("bluetooth")
        || low.contains("inputmethod")
        || low.contains("webview")
        || low.contains("settings")
        || low.contains("sso")
    {
        return true;
    }
    false
}

fn is_known_bloat_or_ad_package(pkg: &str) -> bool {
    STOCK_LAUNCHERS.contains(&pkg)
        || BOOT_AD_PACKAGES.contains(&pkg)
        || REDOWNLOAD_DAEMONS.contains(&pkg)
        || pkg == "com.jio.tr069client"
}

fn safe_uninstall_apps(adb_bin: &Path, target: &str) {
    println!("\nSelect package list mode:");
    println!("  [1] User / ADB-Sideloaded Apps Only (Recommended - pm list packages -3)");
    println!("  [2] All Non-Core & Partner Apps (Includes preloaded apps like ETV, OTT apps, etc.)");
    print!("Select mode (1 or 2, default=1): ");
    let _ = io::stdout().flush();
    let mode = read_line_trimmed();

    println!("[*] Fetching package list from {} (filtering out core OS components)...", target);
    let raw = if mode == "2" {
        adb_shell(adb_bin, target, &["pm", "list", "packages"])
    } else {
        adb_shell(adb_bin, target, &["pm", "list", "packages", "-3"])
    };

    let mut packages: Vec<String> = raw
        .lines()
        .filter_map(|line| line.strip_prefix("package:"))
        .map(|s| s.trim().to_string())
        .filter(|pkg| !pkg.is_empty() && !is_protected_system_package(pkg))
        .collect();

    packages.sort();
    packages.dedup();

    if packages.is_empty() {
        println!("[!] No removable packages found in this view.");
        return;
    }

    println!("\nRemovable Packages on {} (Core System Components Excluded):", target);
    for (idx, pkg) in packages.iter().enumerate() {
        println!("  [{:>2}] {}", idx + 1, pkg);
    }

    print!("\nEnter number(s) to uninstall (e.g. 1 or 1,3,5 — or 0 to cancel): ");
    let _ = io::stdout().flush();
    let input = read_line_trimmed();
    if input == "0" || input.is_empty() {
        println!("[*] Cancelled.");
        return;
    }

    for part in input.split(',') {
        if let Ok(n) = part.trim().parse::<usize>() {
            if n >= 1 && n <= packages.len() {
                let pkg = &packages[n - 1];
                println!("[*] Unbinding & locking '{}' (can be restored via Option 4 if system app)...", pkg);
                let status = neutralize_pkg(adb_bin, target, pkg);
                println!("    -> {}: {}", pkg, status);
            }
        }
    }
}

fn restore_uninstalled_system_packages(adb_bin: &Path, target: &str) {
    println!("[*] Scanning {} for unbound (uninstalled for user 0) or disabled system packages...", target);

    let all_with_uninstalled = adb_shell(adb_bin, target, &["pm", "list", "packages", "-u"]);
    let active_installed = adb_shell(adb_bin, target, &["pm", "list", "packages"]);
    let disabled_raw = adb_shell(adb_bin, target, &["pm", "list", "packages", "-d"]);

    let active_set: HashSet<String> = active_installed
        .lines()
        .filter_map(|l| l.strip_prefix("package:"))
        .map(|s| s.trim().to_string())
        .collect();

    let disabled_set: HashSet<String> = disabled_raw
        .lines()
        .filter_map(|l| l.strip_prefix("package:"))
        .map(|s| s.trim().to_string())
        .collect();

    let mut restorable: Vec<(String, &'static str)> = Vec::new();

    for line in all_with_uninstalled.lines() {
        if let Some(pkg) = line.strip_prefix("package:") {
            let pkg = pkg.trim().to_string();
            if pkg.is_empty() {
                continue;
            }
            if !active_set.contains(&pkg) {
                restorable.push((pkg, "UNBOUND (uninstalled --user 0)"));
            } else if disabled_set.contains(&pkg) {
                restorable.push((pkg, "DISABLED / SUSPENDED"));
            }
        }
    }

    restorable.sort_by(|a, b| a.0.cmp(&b.0));
    restorable.dedup_by(|a, b| a.0 == b.0);

    if restorable.is_empty() {
        println!("[+] No uninstalled or disabled system packages found! All firmware packages are active.");
        return;
    }

    println!("\nUninstalled / Disabled System & Preloaded Packages on {}:", target);
    for (idx, (pkg, state)) in restorable.iter().enumerate() {
        let tag = if is_known_bloat_or_ad_package(pkg) {
            "[KNOWN BLOAT/AD]"
        } else {
            "[SYSTEM / APP]  "
        };
        println!("  [{:>2}] {} {:<42} ({})", idx + 1, tag, pkg, state);
    }

    println!("\n  [S] Restore ALL non-bloat [SYSTEM / APP] packages automatically");
    print!("Enter package number(s) to restore (e.g. 1 or 1,4 — or S, or 0 to cancel): ");
    let _ = io::stdout().flush();
    let input = read_line_trimmed();
    if input == "0" || input.is_empty() {
        println!("[*] Cancelled.");
        return;
    }

    if input.eq_ignore_ascii_case("s") {
        for (pkg, _) in &restorable {
            if !is_known_bloat_or_ad_package(pkg) {
                restore_single_package(adb_bin, target, pkg);
            }
        }
        return;
    }

    for part in input.split(',') {
        if let Ok(n) = part.trim().parse::<usize>() {
            if n >= 1 && n <= restorable.len() {
                let pkg = &restorable[n - 1].0;
                restore_single_package(adb_bin, target, pkg);
            }
        }
    }
}

fn restore_single_package(adb_bin: &Path, target: &str, pkg: &str) {
    println!("[*] Re-binding & enabling '{}' for User 0...", pkg);
    let r1 = adb_shell(adb_bin, target, &["cmd", "package", "install-existing", pkg]);
    let r2 = adb_shell(adb_bin, target, &["pm", "unsuspend", "--user", "0", pkg]);
    let r3 = adb_shell(adb_bin, target, &["pm", "enable", pkg]);
    println!("    -> install-existing: {} | unsuspend: {} | enable: {}", r1, r2, r3);
}

fn install_and_supercharge_tvos(adb_bin: &Path, target: &str, runtime_dir: &Path) {
    let tvos_apk = runtime_dir.join("tvOS-Launcher.apk");
    println!("[*] Sideloading bundled tvOS-Launcher.apk onto {}...", target);
    let (ok, out, err) = adb_exec(
        adb_bin,
        &["-s", target, "install", "-r", "-d", "-g", tvos_apk.to_str().unwrap()],
    );
    if ok || out.contains("Success") {
        println!("[+] tvOS-Launcher.apk installed!");
    } else {
        println!("[!] Install output: {}{}", out.trim(), err.trim());
    }

    println!("[*] Granting tvOS Launcher full privileged permissions, Notification Listener, Admin & Accessibility...");
    let _ = adb_shell(adb_bin, target, &["pm", "grant", TVOS_PKG, "android.permission.WRITE_SECURE_SETTINGS"]);
    let _ = adb_shell(adb_bin, target, &["pm", "grant", TVOS_PKG, "android.permission.PACKAGE_USAGE_STATS"]);
    let _ = adb_shell(adb_bin, target, &["pm", "grant", TVOS_PKG, "android.permission.ACCESS_FINE_LOCATION"]);
    let _ = adb_shell(adb_bin, target, &["pm", "grant", TVOS_PKG, "android.permission.ACCESS_COARSE_LOCATION"]);
    let _ = adb_shell(adb_bin, target, &["appops", "set", TVOS_PKG, "GET_USAGE_STATS", "allow"]);
    let _ = adb_shell(adb_bin, target, &["appops", "set", TVOS_PKG, "SYSTEM_ALERT_WINDOW", "allow"]);
    let _ = adb_shell(adb_bin, target, &["appops", "set", TVOS_PKG, "WRITE_SETTINGS", "allow"]);
    let _ = adb_shell(adb_bin, target, &["appops", "set", TVOS_PKG, "ACCESS_RESTRICTED_SETTINGS", "allow"]);
    let _ = adb_shell(
        adb_bin,
        target,
        &["cmd", "notification", "allow_listener", "com.techgenius.tvoslauncher/com.techgenius.tvoslauncher.MediaWatchService"],
    );
    let _ = adb_shell(
        adb_bin,
        target,
        &["dpm", "set-active-admin", "com.techgenius.tvoslauncher/.SleepReceiver$Admin"],
    );
    let _ = adb_shell(
        adb_bin,
        target,
        &["settings", "put", "secure", "enabled_accessibility_services", "com.techgenius.tvoslauncher/com.techgenius.tvoslauncher.PowerService"],
    );
    let _ = adb_shell(adb_bin, target, &["settings", "put", "secure", "accessibility_enabled", "1"]);
    let _ = adb_shell(adb_bin, target, &["dumpsys", "deviceidle", "whitelist", "+com.techgenius.tvoslauncher"]);

    activate_launcher_component(adb_bin, target, TVOS_HOME_COMPONENT);
}

fn discover_home_launchers(adb_bin: &Path, target: &str) -> Vec<String> {
    let raw = adb_shell(
        adb_bin,
        target,
        &[
            "cmd",
            "package",
            "query-activities",
            "--brief",
            "-a",
            "android.intent.action.MAIN",
            "-c",
            "android.intent.category.HOME",
        ],
    );

    let mut components: Vec<String> = raw
        .lines()
        .map(|l| l.trim())
        .filter(|l| l.contains('/') && !l.starts_with("priority") && !l.starts_with("preferred"))
        .map(|l| l.split_whitespace().last().unwrap_or(l).to_string())
        .filter(|c| {
            let low = c.to_ascii_lowercase();
            !low.starts_with("com.jio.stb.screens/")
                && !low.starts_with("com.iwedia.ui.rjio.launcher.reliance/")
                && !low.contains("com.android.settings")
                && !low.contains("com.android.tv.settings")
        })
        .collect();

    components.sort();
    components.dedup();
    components
}

fn change_launcher(adb_bin: &Path, target: &str) {
    println!("[*] Scanning installed custom TV launchers on {}...", target);
    let mut components = discover_home_launchers(adb_bin, target);

    println!("\nInstalled Custom Launchers:");
    if components.is_empty() {
        println!("  (No third-party launchers detected yet)");
    } else {
        for (i, comp) in components.iter().enumerate() {
            println!("  [{}] {}", i + 1, comp);
        }
    }
    println!("  [I] Install a new Launcher .apk file right now & set it as Default Home");
    println!("  [M] Enter a custom Launcher Component manually (package/.Activity)");

    print!("\nChoose an option (or 0 to cancel): ");
    let _ = io::stdout().flush();
    let sel = read_line_trimmed();
    if sel == "0" || sel.is_empty() {
        return;
    }

    if sel.eq_ignore_ascii_case("i") {
        sideload_apk(adb_bin, target);
        components = discover_home_launchers(adb_bin, target);
        if components.is_empty() {
            println!("[!] Could not detect a HOME activity in the installed APK.");
            return;
        }
        println!("\nUpdated Custom Launchers:");
        for (i, comp) in components.iter().enumerate() {
            println!("  [{}] {}", i + 1, comp);
        }
        print!("Select launcher number to activate (1-{}): ", components.len());
        let _ = io::stdout().flush();
        let new_sel = read_line_trimmed();
        if let Ok(idx) = new_sel.parse::<usize>() {
            if idx >= 1 && idx <= components.len() {
                activate_launcher_component(adb_bin, target, &components[idx - 1]);
            }
        }
        return;
    }

    let chosen_component = if sel.eq_ignore_ascii_case("m") {
        print!("Enter ComponentName (e.g. com.techgenius.tvoslauncher/.MainActivity): ");
        let _ = io::stdout().flush();
        read_line_trimmed()
    } else if let Ok(idx) = sel.parse::<usize>() {
        if idx >= 1 && idx <= components.len() {
            components[idx - 1].clone()
        } else {
            println!("[!] Invalid selection.");
            return;
        }
    } else {
        println!("[!] Invalid selection.");
        return;
    };

    activate_launcher_component(adb_bin, target, &chosen_component);
}

fn activate_launcher_component(adb_bin: &Path, target: &str, component: &str) {
    let pkg_name = component.split('/').next().unwrap_or("");
    if !pkg_name.is_empty() {
        let _ = adb_shell(adb_bin, target, &["pm", "unsuspend", "--user", "0", pkg_name]);
        let _ = adb_shell(adb_bin, target, &["pm", "enable", pkg_name]);
        let _ = adb_shell(adb_bin, target, &["pm", "grant", pkg_name, "android.permission.WRITE_SECURE_SETTINGS"]);
        let _ = adb_shell(adb_bin, target, &["dumpsys", "deviceidle", "whitelist", &format!("+{}", pkg_name)]);
    }

    fix_home_button(adb_bin, target);

    let res = adb_shell(adb_bin, target, &["cmd", "package", "set-home-activity", component]);
    println!("[+] Default Home Launcher locked to: {} ({})", component, res);

    let _ = adb_shell(adb_bin, target, &["am", "start", "-n", component]);
}

fn fix_home_button(adb_bin: &Path, target: &str) {
    println!("[*] Disabling & removing stock Jio Launchers so Home button never reverts...");
    for pkg in STOCK_LAUNCHERS {
        let status = neutralize_pkg(adb_bin, target, pkg);
        println!("    -> {}: {}", pkg, status);
    }

    let raw = adb_shell(
        adb_bin,
        target,
        &[
            "cmd",
            "package",
            "query-activities",
            "--brief",
            "-a",
            "android.intent.action.MAIN",
            "-c",
            "android.intent.category.HOME",
        ],
    );
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some((pkg, _)) = trimmed.split_once('/') {
            let pkg_clean = pkg.split_whitespace().last().unwrap_or(pkg);
            if pkg_clean.contains("jio") || pkg_clean.contains("iwedia") {
                let status = neutralize_pkg(adb_bin, target, pkg_clean);
                println!("    -> {} (dynamic Home resolver): {}", pkg_clean, status);
            }
        }
    }

    let custom_launchers = discover_home_launchers(adb_bin, target);
    if let Some(first) = custom_launchers.first() {
        let _ = adb_shell(adb_bin, target, &["cmd", "package", "set-home-activity", first]);
        println!("[+] Confirmed active custom Home launcher: {}", first);
    }
}

fn remove_boot_ads(adb_bin: &Path, target: &str) {
    println!("[*] Removing startup boot ads, ad injectors & promo screensavers...");
    for pkg in BOOT_AD_PACKAGES {
        let status = neutralize_pkg(adb_bin, target, pkg);
        println!("    -> {}: {}", pkg, status);
    }

    let _ = adb_shell(adb_bin, target, &["settings", "put", "secure", "screensaver_enabled", "0"]);
    let _ = adb_shell(adb_bin, target, &["settings", "put", "secure", "screensaver_activate_on_dock", "0"]);
    let _ = adb_shell(adb_bin, target, &["settings", "put", "secure", "screensaver_activate_on_sleep", "0"]);
    println!("[+] Boot ads and promo services completely removed!");
}

fn block_auto_redownloads(adb_bin: &Path, target: &str) {
    println!("[*] Blocking silent app auto-redownloads (ETV, FOTA & background push daemons)...");

    let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "ota_disable_automatic_update", "1"]);
    let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "package_verifier_enable", "0"]);

    let is_jhs = !adb_shell(adb_bin, target, &["pm", "list", "packages", "com.iwedia"]).is_empty();

    for pkg in REDOWNLOAD_DAEMONS {
        let status = neutralize_pkg(adb_bin, target, pkg);
        println!("    -> {}: {}", pkg, status);
    }

    if !is_jhs {
        let status = neutralize_pkg(adb_bin, target, "com.jio.tr069client");
        println!("    -> com.jio.tr069client: {}", status);
    }

    for keyword in ["etv", "eenadu"] {
        let matches = adb_shell(adb_bin, target, &["pm", "list", "packages", keyword]);
        for line in matches.lines() {
            if let Some(pkg) = line.strip_prefix("package:") {
                let pkg = pkg.trim();
                if !pkg.is_empty() && !is_protected_system_package(pkg) {
                    let status = neutralize_pkg(adb_bin, target, pkg);
                    println!("    -> Blocked ETV package '{}': {}", pkg, status);
                }
            }
        }
    }

    print!("\nWant to permanently block another app by keyword (e.g. hotstar, zee5, sonyliv)? Press Enter to skip: ");
    let _ = io::stdout().flush();
    let kw = read_line_trimmed().to_ascii_lowercase();
    if !kw.is_empty() {
        let matches = adb_shell(adb_bin, target, &["pm", "list", "packages", &kw]);
        for line in matches.lines() {
            if let Some(pkg) = line.strip_prefix("package:") {
                let pkg = pkg.trim();
                if !pkg.is_empty() && !is_protected_system_package(pkg) {
                    let status = neutralize_pkg(adb_bin, target, pkg);
                    println!("    -> Permanently blocked '{}': {}", pkg, status);
                }
            }
        }
    }

    println!("[+] Auto-redownload shield active!");
}

fn boost_performance_and_ram(adb_bin: &Path, target: &str) {
    println!("\nSelect Animation & UI Speed Profile:");
    println!("  [1] 0.5x Fast & Smooth (Recommended for tvOS Launcher)");
    println!("  [2] 0.0x Instant (Zero animations, maximum snappiness)");
    println!("  [3] 1.0x Stock Default");
    print!("Select profile (1-3, default=1): ");
    let _ = io::stdout().flush();
    let choice = read_line_trimmed();

    let scale = match choice.as_str() {
        "2" => "0.0",
        "3" => "1.0",
        _ => "0.5",
    };

    println!("[*] Setting window, transition & animator scales to {}x...", scale);
    let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "window_animation_scale", scale]);
    let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "transition_animation_scale", scale]);
    let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "animator_duration_scale", scale]);

    println!("[*] Trimming package caches & killing idle background tasks...");
    let _ = adb_shell(adb_bin, target, &["pm", "trim-caches", "999999999999"]);
    let _ = adb_shell(adb_bin, target, &["am", "kill-all"]);

    let mem = adb_shell(adb_bin, target, &["cat", "/proc/meminfo"]);
    for line in mem.lines().take(3) {
        println!("    {}", line);
    }
    println!("[+] Performance & RAM optimization complete!");
}

fn configure_private_dns(adb_bin: &Path, target: &str) {
    println!("\nConfigure Private DNS (Android 9+ DoT):");
    println!("  [1] AdGuard DNS (dns.adguard-dns.com) — Blocks ads & trackers system-wide");
    println!("  [2] Cloudflare DNS (one.one.one.one) — Fast, unfiltered resolution");
    println!("  [3] Google DNS (dns.google)");
    println!("  [4] Off / Router Default");
    print!("Select DNS option (1-4): ");
    let _ = io::stdout().flush();
    let choice = read_line_trimmed();

    let host = match choice.as_str() {
        "1" => Some("dns.adguard-dns.com"),
        "2" => Some("one.one.one.one"),
        "3" => Some("dns.google"),
        "4" => None,
        _ => return,
    };

    if let Some(h) = host {
        let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "private_dns_mode", "hostname"]);
        let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "private_dns_specifier", h]);
        println!("[+] Private DNS set to '{}'!", h);
    } else {
        let _ = adb_shell(adb_bin, target, &["settings", "put", "global", "private_dns_mode", "off"]);
        println!("[+] Private DNS turned OFF (using router/DHCP DNS).");
    }
}

fn backup_installed_apk(adb_bin: &Path, target: &str) {
    let raw = adb_shell(adb_bin, target, &["pm", "list", "packages", "-3"]);
    let mut pkgs: Vec<String> = raw
        .lines()
        .filter_map(|l| l.strip_prefix("package:"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    pkgs.sort();

    if pkgs.is_empty() {
        println!("[!] No third-party packages found to extract.");
        return;
    }

    println!("\nInstalled User Packages:");
    for (i, p) in pkgs.iter().enumerate() {
        println!("  [{:>2}] {}", i + 1, p);
    }
    print!("\nSelect package number to extract .apk to Host PC (or 0 to cancel): ");
    let _ = io::stdout().flush();
    let sel = read_line_trimmed();
    let idx = match sel.parse::<usize>() {
        Ok(n) if n >= 1 && n <= pkgs.len() => n - 1,
        _ => return,
    };

    let pkg = &pkgs[idx];
    let path_out = adb_shell(adb_bin, target, &["pm", "path", pkg]);
    if let Some(remote_apk) = path_out.lines().next().and_then(|l| l.strip_prefix("package:")) {
        let _ = fs::create_dir_all("apk_backups");
        let dest = format!("apk_backups/{}.apk", pkg);
        println!("[*] Pulling {} -> {}...", remote_apk.trim(), dest);
        let (ok, out, err) = adb_exec(adb_bin, &["-s", target, "pull", remote_apk.trim(), &dest]);
        if ok {
            println!("[+] Saved APK backup to '{}'!", dest);
        } else {
            println!("[!] Pull output: {}{}", out, err);
        }
    }
}

fn remote_helper_menu(adb_bin: &Path, target: &str) {
    println!("\nRemote Control & Diagnostic Helper:");
    println!("  [1] Open Hidden Stock Android Settings on TV");
    println!("  [2] Open Developer Options directly on TV");
    println!("  [3] Type Text / Password onto TV Screen from Host Keyboard");
    println!("  [4] Capture TV Screenshot (saves stb_screenshot.png on Host)");
    print!("Select action (1-4): ");
    let _ = io::stdout().flush();
    let act = read_line_trimmed();

    match act.as_str() {
        "1" => {
            let _ = adb_shell(adb_bin, target, &["am", "start", "-a", "android.settings.SETTINGS"]);
            println!("[+] Launched Stock Android Settings on TV!");
        }
        "2" => {
            let _ = adb_shell(adb_bin, target, &["am", "start", "-a", "android.settings.APPLICATION_DEVELOPMENT_SETTINGS"]);
            println!("[+] Launched Developer Options on TV!");
        }
        "3" => {
            print!("Enter text to type onto TV (spaces are supported): ");
            let _ = io::stdout().flush();
            let txt = read_line_trimmed().replace(' ', "%s");
            if !txt.is_empty() {
                let _ = adb_shell(adb_bin, target, &["input", "text", &txt]);
                println!("[+] Sent text to TV!");
            }
        }
        "4" => {
            let _ = adb_shell(adb_bin, target, &["screencap", "-p", "/sdcard/stb_screen.png"]);
            let (ok, _, _) = adb_exec(adb_bin, &["-s", target, "pull", "/sdcard/stb_screen.png", "stb_screenshot.png"]);
            let _ = adb_shell(adb_bin, target, &["rm", "/sdcard/stb_screen.png"]);
            if ok {
                println!("[+] Saved TV screenshot to 'stb_screenshot.png'!");
            }
        }
        _ => {}
    }
}

fn reboot_menu(adb_bin: &Path, target: &str) {
    println!("\nPower & Reboot Options:");
    println!("  [1] Normal Reboot STB");
    println!("  [2] Restart Wireless ADB Daemon (tcpip 5555)");
    println!("  [3] Reboot to Recovery Mode");
    println!("  [4] Reboot to Bootloader / Fastboot");
    print!("Select option (1-4, or 0 to cancel): ");
    let _ = io::stdout().flush();
    let c = read_line_trimmed();

    match c.as_str() {
        "1" => {
            let _ = adb_exec(adb_bin, &["-s", target, "reboot"]);
            println!("[+] Reboot command sent!");
        }
        "2" => {
            let _ = adb_exec(adb_bin, &["-s", target, "tcpip", "5555"]);
            println!("[+] Restarted ADB on TCP :5555!");
        }
        "3" => {
            let _ = adb_exec(adb_bin, &["-s", target, "reboot", "recovery"]);
            println!("[+] Rebooting into Recovery...");
        }
        "4" => {
            let _ = adb_exec(adb_bin, &["-s", target, "reboot", "bootloader"]);
            println!("[+] Rebooting into Bootloader...");
        }
        _ => {}
    }
}
