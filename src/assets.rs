use anyhow::Result;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
pub const ADB_EXE: &[u8] = include_bytes!("../adb.exe");
#[cfg(target_os = "windows")]
pub const ADB_WIN_API: &[u8] = include_bytes!("../AdbWinApi.dll");
#[cfg(target_os = "windows")]
pub const ADB_WIN_USB_API: &[u8] = include_bytes!("../AdbWinUsbApi.dll");

#[cfg(not(target_os = "windows"))]
pub const ADB_LINUX: &[u8] = include_bytes!("../adb_linux");

pub const TVOS_LAUNCHER_APK: &[u8] = include_bytes!("../tvOS-Launcher.apk");
pub const SHEVERY_APK: &[u8] = include_bytes!("../shevery-release.apk");
pub const ACS_CHAIN_PEM: &[u8] = include_bytes!("../tls/acs_chain.pem");

pub fn adb_bin_path(work_dir: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        work_dir.join("adb.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let bundled = work_dir.join("adb");
        if bundled.exists() {
            bundled
        } else {
            PathBuf::from("adb")
        }
    }
}

pub fn extract_adb_only() -> Result<PathBuf> {
    let work_dir = env::temp_dir().join("ferrostb_runtime");
    fs::create_dir_all(&work_dir)?;

    #[cfg(target_os = "windows")]
    {
        let files: &[(&str, &[u8])] = &[
            ("adb.exe", ADB_EXE),
            ("AdbWinApi.dll", ADB_WIN_API),
            ("AdbWinUsbApi.dll", ADB_WIN_USB_API),
        ];

        for (name, bytes) in files {
            let path = work_dir.join(name);
            if !path.exists() {
                let _ = fs::write(&path, bytes);
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if ADB_LINUX.len() > 1024 {
            let path = work_dir.join("adb");
            let _ = fs::write(&path, ADB_LINUX);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = fs::metadata(&path) {
                    let mut perms = meta.permissions();
                    perms.set_mode(0o755);
                    let _ = fs::set_permissions(&path, perms);
                }
            }
        }
    }

    Ok(work_dir)
}

#[allow(dead_code)]
pub fn extract_runtime_assets() -> Result<PathBuf> {
    let work_dir = extract_adb_only()?;

    let apks: &[(&str, &[u8])] = &[
        ("tvOS-Launcher.apk", TVOS_LAUNCHER_APK),
        ("shevery-release.apk", SHEVERY_APK),
    ];

    for (name, bytes) in apks {
        let path = work_dir.join(name);
        let _ = fs::write(&path, bytes);
    }

    Ok(work_dir)
}
