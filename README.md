<div align="center">

# FerroSTB & FerroSTB Toolbox

*Tired of unskippable startup ads, locked home screens, and blocked app installs on your Jio Set-Top Box?
FerroSTB is a free, lightweight Windows and Linux app that unlocks your Jio STB in under 3 minutes—no coding, no Docker, and no complicated setup required. Run one program on your PC, restart your box, and get a clean, fast Android TV home screen where you can sideload any APK and delete unwanted bloatware.*

**Zero-Dependency Native Rust TR-069 Provisioner, Ad-Blocker & Post-Unlock Management Suite for Jio Set-Top Boxes**

<p>
  <img src="https://img.shields.io/badge/Version-v0.7.0-0969DA?style=flat-square" alt="Version v0.7.0" />
  <img src="https://img.shields.io/badge/Built%20With-Rust%202021-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Built With Rust" />
  <img src="https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-2EA043?style=flat-square" alt="Platform Windows and Linux" />
  <img src="https://img.shields.io/badge/Target-Jio%20STB%20%2F%20Android%20TV-8957E5?style=flat-square&logo=android&logoColor=white" alt="Target Jio STB" />
  <img src="https://img.shields.io/github/v/release/eadxmz/ferrostb?style=flat-square&color=1F6FEB&label=Release" alt="Latest Release" />
  <img src="https://img.shields.io/github/license/eadxmz/ferrostb?style=flat-square&color=6E7781&label=License" alt="License" />
</p>

</div>

---

## Overview

**FerroSTB** is a standalone, ground-up Rust provisioning engine (`ferrostb` / `ferrostb.exe`) paired with an interactive post-unlock management utility (`ferrostb-toolbox` / `ferrostb-toolbox.exe`). It eliminates legacy multi-container stacks (Docker, MongoDB, Node.js, Python proxies, and external DNS daemons) by compiling the entire network spoofing, TR-069 CWMP TLS server, and ADB provisioning pipeline into native **Windows** and **Linux** executables with all runtime payloads embedded directly in memory.

> [!NOTE]
> **Supported Hardware & Firmware:**
> * **JMSC200Av2** and **JMSB200A** across firmware builds `22.1.09`, `24.1.39`, `25.1.9`, and earlier.
> * **JHS Series (JHSC200 v1 / JioOS 2.0)** on firmware build `stb-jhcs200-11.3.46`.

> [!TIP]
> **Recovery & Troubleshooting:**
> * **Accidentally removed a system component?** Launch `ferrostb-toolbox` and run **Option 4 (System Package Restore)** to re-bind any unbound or disabled firmware package without resetting the box.
> * **Device misbehaving?** Perform a factory reset on your Jio STB to restore clean firmware defaults.
> * **Wi-Fi connection stalled?** Open network settings on the STB, select **Forget Network**, and reconnect.

> [!IMPORTANT]
> **On-Device APK Installer Restriction:**
> Jio firmware disables the stock Android package installer UI on the Set-Top Box, preventing direct APK installation from on-device file managers. Use **`ferrostb-toolbox` (Option 2)** on Windows or Linux to drag-and-drop and install `.apk` files over wireless ADB.

---

## Release Bundles

Pre-compiled binaries are published automatically to the **[Releases](https://github.com/eadxmz/ferrostb/releases)** page on every update:

<p>
  <img src="https://img.shields.io/badge/Windows%20x64-FerroSTB--Windows--Bundle.zip-0078D4?style=flat-square&logo=windows&logoColor=white" alt="Windows Bundle" />
  <img src="https://img.shields.io/badge/Linux%20x64-FerroSTB--Linux--Bundle.zip%20%7C%20.tar.gz-FCC624?style=flat-square&logo=linux&logoColor=black" alt="Linux Bundle" />
</p>

| Bundle Archive | Target OS | Included Executables |
| :--- | :--- | :--- |
| **`FerroSTB-Windows-Bundle.zip`** | Windows 10 / 11 (`x86_64-pc-windows-msvc`) | `ferrostb.exe`, `ferrostb-toolbox.exe` |
| **`FerroSTB-Linux-Bundle.zip`** / **`.tar.gz`** | Linux (`x86_64-unknown-linux-gnu`) | `ferrostb`, `ferrostb-toolbox` |

---

## Core Provisioning Capabilities (`ferrostb`)

* **Wireless ADB Unlock:** Enables network ADB on TCP port `5555` with extended debug validity through `2037`.
* **tvOS Launcher Installation & Permission Granting:** Sideloads **tvOS Launcher** (`com.techgenius.tvoslauncher`) and automatically configures `WRITE_SECURE_SETTINGS`, `PACKAGE_USAGE_STATS`, Location access, Notification Listener (`MediaWatchService`), Accessibility (`PowerService`), Device Admin (`SleepReceiver$Admin`), and battery optimization whitelist.
* **Permanent Home Button Lock:** Force-stops, suspends, disables, and unbinds `com.jio.stb.screens`, `com.iwedia.ui.rjio.launcher.reliance`, and any dynamic Jio/Iwedia `CATEGORY_HOME` resolvers so the remote **Home** button never reverts to the stock interface.
* **Shevery Privileged Service:** Sideloads `com.shevery` and grants `WRITE_SECURE_SETTINGS`, `DUMP`, `PACKAGE_USAGE_STATS`, background `appops`, and battery whitelist.
* **Startup Boot Ad & Screensaver Purge:** Neutralizes `com.jio.stbadservice`, `com.jio.adc.aka`, `com.jio.adc.embedded`, `com.jio.bioscopecontentservice`, `com.jio.screensaver`, `com.jio.systemstats`, and `com.rjil.podlogservice`.
* **Silent App Auto-Redownload & OTA Shield:** Strips `com.jio.httpupdate`, `com.jio.usbupdate`, `com.jio.fota`, `com.jio.stb.appstore`, `com.jio.stb.downloadmanager`, `com.rjil.jio.stb.commandreceiver`, `com.jio.backgroundscheduler`, and `com.iwedia.*` updaters so removed partner apps do not silently reinstall.

---

## Post-Unlock Management Utility (`ferrostb-toolbox`)

**FerroSTB Toolbox** is an interactive CLI utility with embedded platform-specific ADB binaries (`adb.exe` on Windows, official Google `adb` on Linux) for ongoing device customization:

| Option | Module | Function |
| :--- | :--- | :--- |
| **1** | **Wireless ADB Connect** | Auto-detects active LAN ADB sessions or connects to `<STB_IP>:5555` |
| **2** | **APK Sideloader** | Installs single `.apk` files or batch-installs an entire directory of `.apk` files |
| **3** | **Safe-Mode App Uninstaller** | Lists user/ADB apps (`-3`) or removable partner apps while protecting core Android OS, SSO, and Bluetooth remote packages |
| **4** | **System Package Restore** | Scans unbound (`--user 0`) and disabled firmware packages and restores them via `cmd package install-existing` |
| **5** | **Dynamic Launcher Switcher** | Discovers all installed `CATEGORY_HOME` launchers (or installs a new launcher `.apk`) and locks it as default Home |
| **6** | **tvOS Launcher Supercharger** | Re-installs bundled `tvOS-Launcher.apk` and restores all privileged permissions, listeners, and admin rights |
| **7** | **Home Button Lock** | Disables and unbinds all stock Jio and Iwedia home resolvers |
| **8** | **Boot Ad & Promo Remover** | Removes startup video ads, ad injectors, and idle promo screensavers |
| **9** | **Auto-Redownload Blocker** | Neutralizes silent background app installers and locks out unwanted partner apps (such as ETV or custom keywords) |
| **10** | **Performance & RAM Booster** | Configures `0.5x` or `0.0x` animation scales, trims package caches, and clears background memory |
| **11** | **Private DNS Switcher** | Sets Android 9+ DNS-over-TLS (`dns.adguard-dns.com`, `one.one.one.one`, `dns.google`, or Off) |
| **12** | **APK Extractor / Backup** | Pulls installed user `.apk` packages from the STB into `apk_backups/` on your host machine |
| **13** | **Remote Helper & Screenshot** | Launches hidden Android Settings / Developer Options on TV, sends keyboard text input, or captures a PNG screenshot |
| **14** | **Power & Reboot Menu** | Triggers normal reboot, restarts wireless ADB (`tcpip 5555`), or boots into Recovery / Bootloader |

---

## System Architecture

| Component | Port / Protocol | Implementation Details |
| :--- | :--- | :--- |
| **Asset Embedder (`src/assets.rs`)** | Compile-Time | Embeds platform ADB binaries, `tls/acs_chain.pem`, `tvOS-Launcher.apk`, and `shevery-release.apk` |
| **Host Network & Firewall (`src/main.rs`)** | OS Native | Evicts conflicting port `:53` daemons (`SharedAccess`/`hns` on Windows; `systemd-resolved` on Linux) and configures temporary firewall rules |
| **Selective DNS Spoofer (`src/dns.rs`)** | `UDP :53` | Redirects `acs.oss.jio.com`, NTP, and connectivity checks to the host LAN IP while returning `NXDOMAIN` to unrelated traffic |
| **NTP Time Daemon (`src/ntp.rs`)** | `UDP :123` | Syncs the STB cold-boot clock (`1970`) to current UTC so TLS certificate chains and debug validity checks succeed |
| **Captive Portal Spoofer (`src/captive.rs`)** | `TCP :80` | Returns `HTTP/1.1 204 No Content` while upstream WAN/fiber is disconnected so Android TV keeps Wi-Fi active |
| **Stateless CWMP Server (`src/cwmp.rs`)** | `TCP :443, :7443, :8443, :9443` | OpenSSL (`@SECLEVEL=0` + `RILROOTCA`) server with `/common/v1/network/check` spoofing, shallow tree discovery, and atomic 1-parameter RPCs |
| **Parallel ADB Provisioner (`src/adb.rs`)** | `TCP :5555` | Polls port `5555` concurrently with CWMP, deploys APKs, locks default Home, and purges bloatware |

---

## Prerequisites

1. **A Windows 10/11 or Linux (`x86_64`) PC** connected to the same router as your Jio STB.
2. **Release Bundle** from the **[Releases](https://github.com/eadxmz/ferrostb/releases)** page (`FerroSTB-Windows-Bundle.zip` or `FerroSTB-Linux-Bundle.zip` / `.tar.gz`).
3. **Any empty USB flash drive** (used briefly in Step 3 to open the hidden stock Android Settings menu on the STB).

---

## Step-by-Step Setup Guide

### Step 1: Network Pairing

1. Connect both your Windows/Linux PC and your Jio STB to the **same primary home router**.
2. Do not use **Guest Wi-Fi**, as client isolation blocks LAN communication between devices.

---

### Step 2: Find Your Host PC LAN IPv4 Address

* **On Windows (`cmd`):**
  ```cmd
  ipconfig
  ```
  Note the **IPv4 Address** under your active Wi-Fi or Ethernet adapter (for example, `192.168.31.52`).

* **On Linux (Terminal):**
  ```bash
  ip -4 addr show
  ```
  Note the `inet` IPv4 address under your active `wlan0` or `eth0` interface (for example, `192.168.31.52`).

---

### Step 3: Configure STB Static DNS via USB Storage Breakout

1. Insert any **empty USB flash drive** into the Jio STB USB port.
2. On the STB, open **My Music** $\rightarrow$ **Settings** $\rightarrow$ **Eject USB**.
3. Select **Internal Storage** $\rightarrow$ **Apps** $\rightarrow$ **Settings** (gear icon) $\rightarrow$ **Open**.
4. Go to **Network & Internet** $\rightarrow$ select your connected Wi-Fi network $\rightarrow$ open **IP Settings** (or **Advanced IP Settings**).
5. Note the STB's current **IP Address** (e.g., `192.168.31.181`) and **Gateway** (e.g., `192.168.31.1`) before changing modes.
6. Switch **IP Settings** from **DHCP** to **Static**:
   * **IP Address:** Enter the STB's existing LAN IP (overwrite any gray placeholder such as `192.168.1.128`).
   * **Gateway:** Enter your router's gateway IP (e.g., `192.168.31.1`).
   * **Network prefix length:** `24`
   * **DNS 1:** Enter your **Host PC IPv4 Address** from Step 2 (e.g., `192.168.31.52`).
   * **DNS 2:** Enter the same **Host PC IPv4 Address** so DNS queries cannot leak upstream.
7. Save the configuration and verify the STB reconnects to Wi-Fi.

> [!WARNING]
> If Android clears the IP or Gateway fields and displays placeholder text (`192.168.1.128`) after switching to Static, enter your real subnet IP and Gateway manually so the STB stays on your router's subnet.

---

### Step 4: Isolate WAN / Fiber Connection

1. Unplug the **incoming fiber-optic or WAN Ethernet cable** from the back of your router.
2. Keep the router **powered ON** so local Wi-Fi traffic between your PC and the STB continues uninterrupted without reaching external ACS servers.

---

### Step 5: Launch `ferrostb` & Cold-Reboot the STB

* **On Windows:**
  1. Extract `FerroSTB-Windows-Bundle.zip`.
  2. Right-click **`ferrostb.exe`** and select **Run as administrator**.

* **On Linux:**
  1. Extract `FerroSTB-Linux-Bundle.tar.gz` (or `.zip`):
     ```bash
     tar -xzf FerroSTB-Linux-Bundle.tar.gz && cd FerroSTB-Linux-Bundle
     sudo ./ferrostb
     ```

Once the console displays:

```text
[READY] ALL SERVERS LISTENING!
>>> Unplug the Jio STB power cord, wait 5s, and plug in.
```

Perform a **cold power cycle** on the Jio STB:
1. Unplug the power barrel connector from the back of the STB.
2. Wait **5 full seconds** and plug it back in.
3. `ferrostb` will intercept DNS, sync NTP time, pass the Captive Portal check, execute the TR-069 CWMP sequence, and connect to ADB on port `5555`.

---

### Step 6: Approve USB Debugging on TV

When the **"Allow USB Debugging?"** prompt appears on your TV screen:
1. Check **Always allow from this computer**.
2. Select **OK / Allow** using your remote.

`ferrostb` will complete the installation of **tvOS Launcher** and **Shevery**, disable stock Jio launchers and boot ads, block silent app re-downloads, and launch **tvOS Launcher**.

---

### Step 7: Restore Internet & Customize with `ferrostb-toolbox`

1. Reconnect the **fiber-optic / WAN cable** to your router.
2. On the Jio STB, change Wi-Fi **IP Settings** back from **Static** to **DHCP**.
3. Run **`ferrostb-toolbox.exe`** (Windows) or **`./ferrostb-toolbox`** (Linux) whenever you want to sideload `.apk` files, switch launchers, remove or restore packages, or tune system settings.

---

## Building from Source

To compile both binaries locally using Cargo:

```bash
cargo build --release
```

Output binaries are generated at `target/release/ferrostb` (`.exe`) and `target/release/ferrostb-toolbox` (`.exe`).

---

## Disclaimer

This software is provided for educational and personal device administration purposes on hardware you own or have explicit authorization to manage. See the [LICENSE](LICENSE) file for full license terms.
