#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adb;
mod assets;
mod captive;
mod cwmp;
mod dns;
mod logger;
mod ntp;
mod provisioner;

use anyhow::Result;
use cwmp::StbFamily;
use logger::AppState;
use slint::{
    CloseRequestResponse, ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer,
    SharedString, Timer, TimerMode, VecModel, Weak,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

slint::include_modules!();

fn show_toast(ui_weak: &Weak<MainWindow>, msg: impl Into<String>) {
    let text = msg.into();
    let _ = ui_weak.upgrade_in_event_loop(move |ui| {
        ui.set_toast_message(text.into());
        ui.set_toast_visible(true);
    });
}

fn parse_ip_from_label(label: &str) -> String {
    label
        .split_whitespace()
        .next()
        .unwrap_or(label)
        .trim()
        .to_string()
}

fn populate_lan_ips(ui: &MainWindow, state: &Arc<AppState>) {
    let entries = provisioner::list_lan_ipv4s();
    let mut labels: Vec<SharedString> = entries
        .iter()
        .map(|(name, ip)| SharedString::from(format!("{}  ({})", ip, name)))
        .collect();

    if labels.is_empty() {
        let fallback = provisioner::detect_host_ipv4()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        labels.push(SharedString::from(fallback.clone()));
        state.set_host_ip(fallback.clone());
        ui.set_host_ip(fallback.clone().into());
        ui.set_selected_lan_ip(fallback.into());
    } else {
        let best_ip = entries[0].1.to_string();
        state.set_host_ip(best_ip.clone());
        ui.set_host_ip(best_ip.into());
        ui.set_selected_lan_ip(labels[0].clone());
    }

    ui.set_lan_ip_options(ModelRc::new(VecModel::from(labels)));
}

fn apply_package_filter(ui: &MainWindow, all_pkgs: &[adb::PackageEntry]) {
    let tab = ui.get_app_filter_tab();
    let query = ui.get_pkg_search().to_string().trim().to_ascii_lowercase();

    let items: Vec<PackageItem> = all_pkgs
        .iter()
        .filter(|p| {
            if !query.is_empty() && !p.name.to_ascii_lowercase().contains(&query) {
                return false;
            }
            match tab {
                1 => p.category == "user" && p.status == "active",
                2 => p.status != "active",
                3 => p.category == "bloat",
                _ => p.status == "active" && !p.protected,
            }
        })
        .map(|p| PackageItem {
            name: p.name.clone().into(),
            category: p.category.clone().into(),
            status: p.status.clone().into(),
            is_active: p.status == "active",
        })
        .collect();

    ui.set_filtered_packages(ModelRc::new(VecModel::from(items)));
}

fn trigger_refresh_packages(
    rt: &Arc<tokio::runtime::Runtime>,
    state: &Arc<AppState>,
    pkgs_cache: &Arc<Mutex<Vec<adb::PackageEntry>>>,
    ui_weak: &Weak<MainWindow>,
) {
    let Some(target) = state.active_target() else {
        show_toast(ui_weak, "No STB connected. Enter your STB IP in the Connect banner first.");
        return;
    };

    let cache = pkgs_cache.clone();
    let weak = ui_weak.clone();
    rt.spawn_blocking(move || {
        let list = assets::extract_adb_only()
            .ok()
            .map(|dir| adb::list_packages(&assets::adb_bin_path(&dir), &target))
            .unwrap_or_default();
        let count = list.len() as i32;
        if let Ok(mut guard) = cache.lock() {
            *guard = list.clone();
        }
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_app_count(count);
            apply_package_filter(&ui, &list);
        });
    });
}

fn main() -> Result<()> {
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?,
    );

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
    let state = Arc::new(AppState::new(
        host_ip.clone(),
        jhs_ready,
        jhs_src.clone(),
        auto_target.clone(),
    ));
    let pkgs_cache: Arc<Mutex<Vec<adb::PackageEntry>>> = Arc::new(Mutex::new(Vec::new()));

    let ui = MainWindow::new()?;
    populate_lan_ips(&ui, &state);
    ui.set_jhs_cert_ready(jhs_ready);
    ui.set_jhs_cert_source(jhs_src.into());
    if let Some(ref t) = auto_target {
        ui.set_stb_connected(true);
        ui.set_active_target(t.clone().into());
        ui.set_stb_ip_input(t.split(':').next().unwrap_or(t).into());
    }

    // Window close cleanup
    {
        let st = state.clone();
        ui.window().on_close_requested(move || {
            st.stop_provision();
            provisioner::cleanup_firewall_and_services();
            CloseRequestResponse::HideWindow
        });
    }

    // LAN IPv4 Selection & Refresh
    {
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_select_lan_ip(move |label| {
            let ip = parse_ip_from_label(&label);
            st.set_host_ip(ip.clone());
            if let Some(u) = weak.upgrade() {
                u.set_host_ip(ip.clone().into());
                u.set_toast_message(format!("Active Host LAN IP set to {}", ip).into());
                u.set_toast_visible(true);
            }
        });
    }
    {
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_refresh_lan_ips(move || {
            if let Some(u) = weak.upgrade() {
                populate_lan_ips(&u, &st);
                u.set_toast_message("Refreshed physical LAN adapters (VPNs excluded)".into());
                u.set_toast_visible(true);
            }
        });
    }

    // Start / Stop Provisioning
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_start_provision(move || {
            let fam = weak
                .upgrade()
                .map(|u| {
                    if u.get_selected_family().eq_ignore_ascii_case("jms") {
                        StbFamily::Jms
                    } else {
                        StbFamily::Jhs
                    }
                })
                .unwrap_or(StbFamily::Jhs);
            let st_run = st.clone();
            rt_c.spawn(async move {
                provisioner::run_provisioning_pipeline(fam, st_run).await;
            });
            show_toast(&weak, "Provisioning servers starting...");
        });
    }
    {
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_stop_provision(move || {
            st.stop_provision();
            provisioner::cleanup_firewall_and_services();
            show_toast(&weak, "Provisioning stopped & firewall rules cleaned up");
        });
    }

    // Connect / Auto-Detect STB
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let cache = pkgs_cache.clone();
        let weak = ui.as_weak();
        ui.on_connect_stb(move |ip| {
            let ip_s = ip.trim().to_string();
            if ip_s.is_empty() {
                show_toast(&weak, "Enter a Jio STB IP address first");
                return;
            }
            show_toast(&weak, format!("Connecting to {}...", ip_s));
            let st_c = st.clone();
            let weak_c = weak.clone();
            let rt_inner = rt_c.clone();
            let cache_c = cache.clone();
            rt_c.spawn_blocking(move || {
                let res = assets::extract_adb_only()
                    .map_err(|e| e.to_string())
                    .and_then(|dir| adb::connect_target(&assets::adb_bin_path(&dir), &ip_s));
                match res {
                    Ok(target) => {
                        st_c.set_active_target(Some(target.clone()));
                        show_toast(&weak_c, format!("Connected to {}", target));
                        trigger_refresh_packages(&rt_inner, &st_c, &cache_c, &weak_c);
                    }
                    Err(e) => show_toast(&weak_c, e),
                }
            });
        });
    }
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let cache = pkgs_cache.clone();
        let weak = ui.as_weak();
        ui.on_detect_stb(move || {
            let st_c = st.clone();
            let weak_c = weak.clone();
            let rt_inner = rt_c.clone();
            let cache_c = cache.clone();
            rt_c.spawn_blocking(move || {
                let found = assets::extract_adb_only()
                    .ok()
                    .and_then(|dir| adb::auto_detect_connected_stb(&assets::adb_bin_path(&dir)));
                if let Some(target) = found {
                    st_c.set_active_target(Some(target.clone()));
                    let ip_only = target.split(':').next().unwrap_or(&target).to_string();
                    let _ = weak_c.upgrade_in_event_loop(move |u| {
                        u.set_stb_ip_input(ip_only.into());
                    });
                    show_toast(&weak_c, format!("Auto-detected {}", target));
                    trigger_refresh_packages(&rt_inner, &st_c, &cache_c, &weak_c);
                } else {
                    show_toast(&weak_c, "No authorized STB found in 'adb devices'");
                }
            });
        });
    }

    // Browse & Install APK
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let cache = pkgs_cache.clone();
        let weak = ui.as_weak();
        ui.on_browse_apk(move || {
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            let rt_inner = rt_c.clone();
            let st_c = st.clone();
            let cache_c = cache.clone();
            let weak_c = weak.clone();
            rt_c.spawn(async move {
                let picked = rfd::AsyncFileDialog::new()
                    .set_title("Select Android APK to Sideload")
                    .add_filter("Android Package (*.apk)", &["apk"])
                    .pick_file()
                    .await;
                let Some(handle) = picked else {
                    return;
                };
                let apk_path = handle.path().to_path_buf();
                let fname = apk_path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "app.apk".to_string());
                let path_display = apk_path.to_string_lossy().to_string();
                let _ = weak_c.upgrade_in_event_loop(move |u| {
                    u.set_apk_path_input(path_display.into());
                    u.set_upload_status(format!("Installing {}...", fname).into());
                });
                let res = tokio::task::spawn_blocking(move || {
                    let dir = assets::extract_adb_only().map_err(|e| e.to_string())?;
                    adb::install_apk_file(&assets::adb_bin_path(&dir), &target, &apk_path)
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
                match res {
                    Ok(m) => {
                        let m_c = m.clone();
                        let _ = weak_c.upgrade_in_event_loop(move |u| u.set_upload_status(m_c.into()));
                        show_toast(&weak_c, m);
                        trigger_refresh_packages(&rt_inner, &st_c, &cache_c, &weak_c);
                    }
                    Err(e) => {
                        let e_c = e.clone();
                        let _ = weak_c.upgrade_in_event_loop(move |u| u.set_upload_status(e_c.into()));
                        show_toast(&weak_c, e);
                    }
                }
            });
        });
    }
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let cache = pkgs_cache.clone();
        let weak = ui.as_weak();
        ui.on_install_apk_path(move |raw_path| {
            let clean = raw_path.trim().trim_matches('"').trim_matches('\'').to_string();
            if clean.is_empty() {
                show_toast(&weak, "Enter a valid .apk file path");
                return;
            }
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            show_toast(&weak, "Installing APK...");
            let rt_inner = rt_c.clone();
            let st_c = st.clone();
            let cache_c = cache.clone();
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let res = assets::extract_adb_only()
                    .map_err(|e| e.to_string())
                    .and_then(|dir| {
                        adb::install_apk_file(&assets::adb_bin_path(&dir), &target, &PathBuf::from(&clean))
                    });
                match res {
                    Ok(m) => {
                        let m_c = m.clone();
                        let _ = weak_c.upgrade_in_event_loop(move |u| u.set_upload_status(m_c.into()));
                        show_toast(&weak_c, m);
                        trigger_refresh_packages(&rt_inner, &st_c, &cache_c, &weak_c);
                    }
                    Err(e) => show_toast(&weak_c, e),
                }
            });
        });
    }

    // Packages Refresh / Filter / Uninstall / Restore / Backup
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let cache = pkgs_cache.clone();
        let weak = ui.as_weak();
        ui.on_refresh_packages(move || {
            trigger_refresh_packages(&rt_c, &st, &cache, &weak);
        });
    }
    {
        let cache = pkgs_cache.clone();
        let weak = ui.as_weak();
        ui.on_filter_packages_changed(move || {
            if let (Some(u), Ok(list)) = (weak.upgrade(), cache.lock()) {
                apply_package_filter(&u, &list);
            }
        });
    }
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let cache = pkgs_cache.clone();
        let weak = ui.as_weak();
        ui.on_uninstall_package(move |pkg| {
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            let pkg_s = pkg.to_string();
            show_toast(&weak, format!("Removing {}...", pkg_s));
            let rt_inner = rt_c.clone();
            let st_c = st.clone();
            let cache_c = cache.clone();
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let lines = assets::extract_adb_only()
                    .map(|dir| adb::safe_uninstall(&assets::adb_bin_path(&dir), &target, &[pkg_s]))
                    .unwrap_or_default();
                show_toast(&weak_c, lines.first().cloned().unwrap_or_else(|| "Done".into()));
                trigger_refresh_packages(&rt_inner, &st_c, &cache_c, &weak_c);
            });
        });
    }
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let cache = pkgs_cache.clone();
        let weak = ui.as_weak();
        ui.on_restore_package(move |pkg| {
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            let pkg_s = pkg.to_string();
            show_toast(&weak, format!("Restoring {}...", pkg_s));
            let rt_inner = rt_c.clone();
            let st_c = st.clone();
            let cache_c = cache.clone();
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let lines = assets::extract_adb_only()
                    .map(|dir| adb::restore_packages(&assets::adb_bin_path(&dir), &target, &[pkg_s]))
                    .unwrap_or_default();
                show_toast(&weak_c, lines.first().cloned().unwrap_or_else(|| "Restored".into()));
                trigger_refresh_packages(&rt_inner, &st_c, &cache_c, &weak_c);
            });
        });
    }
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_backup_package(move |pkg| {
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            let pkg_s = pkg.to_string();
            show_toast(&weak, format!("Backing up {}.apk...", pkg_s));
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let res = assets::extract_adb_only()
                    .map_err(|e| e.to_string())
                    .and_then(|dir| adb::backup_apk(&assets::adb_bin_path(&dir), &target, &pkg_s));
                match res {
                    Ok(m) => show_toast(&weak_c, m),
                    Err(e) => show_toast(&weak_c, e),
                }
            });
        });
    }

    // Launchers & Shield Callbacks
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_scan_launchers(move || {
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let list = assets::extract_adb_only()
                    .ok()
                    .map(|dir| adb::discover_home_launchers(&assets::adb_bin_path(&dir), &target))
                    .unwrap_or_default();
                let count = list.len();
                let shared: Vec<SharedString> = list.into_iter().map(Into::into).collect();
                let _ = weak_c.upgrade_in_event_loop(move |u| {
                    u.set_launchers(ModelRc::new(VecModel::from(shared)));
                    u.set_shield_output(format!("Found {} third-party launcher(s).", count).into());
                });
            });
        });
    }
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_activate_launcher(move |comp| {
            let comp_s = comp.trim().to_string();
            if comp_s.is_empty() {
                show_toast(&weak, "Enter a launcher component name");
                return;
            }
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            show_toast(&weak, format!("Activating {}...", comp_s));
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let lines = assets::extract_adb_only()
                    .map(|dir| adb::activate_launcher(&assets::adb_bin_path(&dir), &target, &comp_s))
                    .unwrap_or_default();
                let out = lines.join("\n");
                let _ = weak_c.upgrade_in_event_loop(move |u| u.set_shield_output(out.into()));
                show_toast(&weak_c, "Launcher activated & locked");
            });
        });
    }
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_supercharge_tvos(move || {
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            show_toast(&weak, "Installing & supercharging tvOS Launcher...");
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let lines = assets::extract_runtime_assets()
                    .map(|dir| adb::supercharge_tvos(&assets::adb_bin_path(&dir), &target, &dir))
                    .unwrap_or_default();
                let out = lines.join("\n");
                let _ = weak_c.upgrade_in_event_loop(move |u| u.set_shield_output(out.into()));
                show_toast(&weak_c, "tvOS Launcher installed & supercharged");
            });
        });
    }
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_fix_home(move || {
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            show_toast(&weak, "Neutralizing stock Jio launchers...");
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let lines = assets::extract_adb_only()
                    .map(|dir| adb::fix_home_button(&assets::adb_bin_path(&dir), &target))
                    .unwrap_or_default();
                let out = lines.join("\n");
                let _ = weak_c.upgrade_in_event_loop(move |u| u.set_shield_output(out.into()));
                show_toast(&weak_c, "Stock Jio launchers neutralized & Home locked");
            });
        });
    }
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_remove_ads(move || {
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            show_toast(&weak, "Removing boot ads & promo screensavers...");
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let lines = assets::extract_adb_only()
                    .map(|dir| adb::remove_boot_ads(&assets::adb_bin_path(&dir), &target))
                    .unwrap_or_default();
                let out = lines.join("\n");
                let _ = weak_c.upgrade_in_event_loop(move |u| u.set_shield_output(out.into()));
                show_toast(&weak_c, "Boot ads & promo screensavers removed");
            });
        });
    }
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_block_redownloads(move |kw| {
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            let kw_opt = {
                let trimmed = kw.trim().to_string();
                if trimmed.is_empty() { None } else { Some(trimmed) }
            };
            show_toast(&weak, "Applying auto-redownload shield...");
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let lines = assets::extract_adb_only()
                    .map(|dir| {
                        adb::block_auto_redownloads(
                            &assets::adb_bin_path(&dir),
                            &target,
                            kw_opt.as_deref(),
                        )
                    })
                    .unwrap_or_default();
                let out = lines.join("\n");
                let _ = weak_c.upgrade_in_event_loop(move |u| u.set_shield_output(out.into()));
                show_toast(&weak_c, "Auto-redownload shield applied");
            });
        });
    }

    // Tweaks, Private DNS, Remote, Screenshot, Reboot
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_boost_perf(move |scale| {
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            let scale_s = scale.to_string();
            show_toast(&weak, format!("Setting animation scale to {}x...", scale_s));
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let lines = assets::extract_adb_only()
                    .map(|dir| adb::boost_performance(&assets::adb_bin_path(&dir), &target, &scale_s))
                    .unwrap_or_default();
                show_toast(&weak_c, lines.first().cloned().unwrap_or_else(|| "Boost complete".into()));
            });
        });
    }
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_set_private_dns(move |host| {
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            let host_opt = {
                let t = host.trim().to_string();
                if t.is_empty() { None } else { Some(t) }
            };
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let msg = assets::extract_adb_only()
                    .map(|dir| adb::set_private_dns(&assets::adb_bin_path(&dir), &target, host_opt.as_deref()))
                    .unwrap_or_else(|e| e.to_string());
                show_toast(&weak_c, msg);
            });
        });
    }
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_remote_action(move |action, text| {
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            let act = action.to_string();
            let txt_opt = {
                let t = text.trim().to_string();
                if t.is_empty() { None } else { Some(t) }
            };
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let res = assets::extract_adb_only()
                    .map_err(|e| e.to_string())
                    .and_then(|dir| adb::remote_action(&assets::adb_bin_path(&dir), &target, &act, txt_opt.as_deref()));
                match res {
                    Ok(m) => show_toast(&weak_c, m),
                    Err(e) => show_toast(&weak_c, e),
                }
            });
        });
    }
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_capture_screenshot(move || {
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            show_toast(&weak, "Capturing TV screen...");
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let res = assets::extract_adb_only()
                    .map_err(|e| e.to_string())
                    .and_then(|dir| adb::capture_screenshot_bytes(&assets::adb_bin_path(&dir), &target))
                    .and_then(|png| {
                        let rgba = image::load_from_memory(&png)
                            .map_err(|e| e.to_string())?
                            .to_rgba8();
                        let (w, h) = rgba.dimensions();
                        Ok((rgba.into_raw(), w, h))
                    });
                match res {
                    Ok((raw, w, h)) => {
                        let _ = weak_c.upgrade_in_event_loop(move |u| {
                            let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&raw, w, h);
                            u.set_screenshot_image(Image::from_rgba8(buf));
                            u.set_has_screenshot(true);
                            u.set_toast_message("Screenshot captured!".into());
                            u.set_toast_visible(true);
                        });
                    }
                    Err(e) => show_toast(&weak_c, e),
                }
            });
        });
    }
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_reboot_stb(move |mode| {
            let Some(target) = st.active_target() else {
                show_toast(&weak, "Connect to your Jio STB first");
                return;
            };
            let mode_s = mode.to_string();
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let res = assets::extract_adb_only()
                    .map_err(|e| e.to_string())
                    .and_then(|dir| adb::reboot_action(&assets::adb_bin_path(&dir), &target, &mode_s));
                match res {
                    Ok(m) => show_toast(&weak_c, m),
                    Err(e) => show_toast(&weak_c, e),
                }
            });
        });
    }

    // Advanced ADB Terminal
    {
        let rt_c = rt.clone();
        let st = state.clone();
        let weak = ui.as_weak();
        ui.on_run_term_cmd(move |cmd| {
            let cmd_s = cmd.trim().to_string();
            if cmd_s.is_empty() {
                return;
            }
            if let Some(u) = weak.upgrade() {
                let mut cur = u.get_term_output().to_string();
                cur.push_str(&format!("\n$ adb {}\n", cmd_s));
                u.set_term_output(cur.into());
                u.set_term_input("".into());
            }
            let target = st.active_target();
            let weak_c = weak.clone();
            rt_c.spawn_blocking(move || {
                let res = assets::extract_adb_only()
                    .map(|dir| adb::run_raw_adb_command(&assets::adb_bin_path(&dir), target.as_deref(), &cmd_s));
                let chunk = match res {
                    Ok(r) => {
                        let mut out = String::new();
                        if !r.stdout.is_empty() {
                            out.push_str(&r.stdout);
                        }
                        if !r.stderr.is_empty() {
                            out.push_str(&format!("[stderr] {}", r.stderr));
                        }
                        if out.is_empty() {
                            out.push_str(&format!("(exit {})\n", r.exit_code));
                        }
                        out
                    }
                    Err(e) => format!("[error] {}\n", e),
                };
                let _ = weak_c.upgrade_in_event_loop(move |u| {
                    let mut cur = u.get_term_output().to_string();
                    cur.push_str(&chunk);
                    u.set_term_output(cur.into());
                });
            });
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_clear_term(move || {
            if let Some(u) = weak.upgrade() {
                u.set_term_output("".into());
            }
        });
    }

    // Toast dismiss & Clean Exit
    {
        let weak = ui.as_weak();
        ui.on_dismiss_toast(move || {
            if let Some(u) = weak.upgrade() {
                u.set_toast_visible(false);
            }
        });
    }
    {
        let st = state.clone();
        ui.on_clean_exit(move || {
            st.stop_provision();
            provisioner::cleanup_firewall_and_services();
            let _ = slint::quit_event_loop();
        });
    }

    // Live State Sync Timer (700ms)
    let poll_timer = Timer::default();
    {
        let st = state.clone();
        let weak = ui.as_weak();
        poll_timer.start(TimerMode::Repeated, Duration::from_millis(700), move || {
            let Some(u) = weak.upgrade() else {
                return;
            };
            let snap = st.snapshot();

            u.set_host_ip(snap.host_ip.into());
            u.set_jhs_cert_ready(snap.jhs_cert_ready);
            u.set_jhs_cert_source(snap.jhs_cert_source.into());
            u.set_prov_status(snap.status.into());
            u.set_step_label(snap.step_label.into());
            u.set_current_step(snap.current_step as i32);
            u.set_dns_hits(snap.dns_spoof_count as i32);
            u.set_detected_stb_ip(
                snap.detected_stb_ip
                    .map(|ip| format!("STB: {}", ip))
                    .unwrap_or_else(|| "Waiting for STB...".to_string())
                    .into(),
            );

            let bound = [snap.dns_bound, snap.ntp_bound, snap.http_bound, snap.cwmp_bound]
                .into_iter()
                .filter(|&b| b)
                .count();
            u.set_ports_label(
                if bound > 0 {
                    format!("{}/4 Active", bound)
                } else {
                    "Idle".to_string()
                }
                .into(),
            );

            if let Some(ref target) = snap.active_target {
                u.set_stb_connected(true);
                u.set_active_target(target.clone().into());
            } else {
                u.set_stb_connected(false);
                u.set_active_target("".into());
            }

            if !snap.success_summary.is_empty() {
                let bullets = snap
                    .success_summary
                    .iter()
                    .map(|s| format!("• {}", s))
                    .collect::<Vec<_>>()
                    .join("\n");
                u.set_success_summary_text(bullets.into());
            }

            if let Some(ref fail) = snap.failure {
                u.set_failure_title(format!("Failed at [{}]: {}", fail.stage, fail.title).into());
                u.set_failure_reason(fail.reason.clone().into());
                let fixes = fail
                    .fixes
                    .iter()
                    .map(|f| format!("• {}", f))
                    .collect::<Vec<_>>()
                    .join("\n");
                u.set_failure_fixes_text(fixes.into());
            }

            if !snap.logs.is_empty() {
                let text = snap
                    .logs
                    .iter()
                    .map(|l| format!("{} [{}] {}", l.ts, l.stage, l.message))
                    .collect::<Vec<_>>()
                    .join("\n");
                u.set_logs_text(text.into());
            }
        });
    }

    ui.run()?;
    state.stop_provision();
    provisioner::cleanup_firewall_and_services();
    Ok(())
}
