<div align="center">

# FerroSTB & FerroSTB Toolbox

**Zero-Dependency Native Rust TR-069 Provisioner, Ad-Blocker & Post-Unlock Management Suite for Jio Set-Top Boxes**

[![Version](https://img.shields.io/badge/VERSION-v0.7.0-f97316?style=for-the-badge&logo=rust&logoColor=white)](#)
[![Platform](https://img.shields.io/badge/PLATFORM-WINDOWS_10_%7C_11_(x64)-0284c7?style=for-the-badge&logo=windows&logoColor=white)](#)
[![Architecture](https://img.shields.io/badge/BINARY-SINGLE--EXE_STANDALONE-10b981?style=for-the-badge)](#)
[![Launcher](https://img.shields.io/badge/DEFAULT_UI-tvOS_LAUNCHER_%2B_SHEVERY-6366f1?style=for-the-badge)](#)

</div>

---

## Overview

**FerroSTB** is a standalone, ground-up Rust provisioning engine (`ferrostb.exe`) paired with an interactive post-unlock management utility (`ferrostb-toolbox.exe`). It eliminates legacy multi-container setups (Docker, MongoDB, Node.js, Python proxies, and external DNS daemons) by compiling the entire network spoofing, TR-069 CWMP TLS server, and ADB provisioning pipeline into native Windows executables with all runtime payloads embedded directly in memory.

> [!NOTE]
> **Supported Hardware & Firmware**
> * **JMSC200Av2** and **JMSB200A** across firmware builds `22.1.09`, `24.1.39`, `25.1.9`, and earlier.
> * **JHS Series (JHSC200 v1 / JioOS 2.0)** on firmware build `stb-jhcs200-11.3.46`.

> [!TIP]
> **System Recovery & Safety**
> * **Accidental App Removal:** Open `ferrostb-toolbox.exe` and select **Option 4 (Restore System Packages)** to re-bind any uninstalled or disabled firmware package (`cmd package install-existing`) without factory resetting the device.
> * **Full Reset:** Performing a standard factory reset restores the Set-Top Box to its stock out-of-box state at any time.
> * **Wi-Fi Reconnection:** If Wi-Fi fails to authenticate after switching IP modes, select **Forget Network**, re-enter your Wi-Fi password, and reconnect.

> [!IMPORTANT]
> **Sideloading APK Files**
> Because the stock Android package installer UI is disabled in firmware, clicking an `.apk` file inside a TV file manager will not install it. Use **`ferrostb-toolbox.exe` (Option 2)** on your PC to drag-and-drop and install `.apk` packages over wireless ADB.

---

## Core Provisioning Capabilities (`ferrostb.exe`)

* ![ADB](https://img.shields.io/badge/ADB_UNLOCK-PORT_5555-10b981?style=flat-square) **Persistent Wireless ADB:** Enables network ADB debugging over TCP port `5555` (validity set through `2037`) and persists global ADB flags across reboots.
* ![Launcher](https://img.shields.io/badge/UI_ENGINE-tvOS_LAUNCHER-6366f1?style=flat-square) **tvOS Launcher Integration:** Sideloads `tvOS-Launcher.apk` (`com.techgenius.tvoslauncher`) and automatically grants `WRITE_SECURE_SETTINGS`, `PACKAGE_USAGE_STATS`, `SYSTEM_ALERT_WINDOW`, Notification Listener (`MediaWatchService`), Accessibility Service (`PowerService`), and Device Admin (`SleepReceiver$Admin`).
* ![Home Lock](https://img.shields.io/badge/HOME_LOCK-STOCK_LAUNCHER_REMOVAL-0284c7?style=flat-square) **Permanent Home Button Lock:** Force-stops, clears, suspends, disables, and unbinds `com.jio.stb.screens` alongside Reliance's high-priority `com.iwedia.ui.rjio.launcher.reliance` home activity so pressing the remote's **Home** button never reverts to the stock interface.
* ![Shevery](https://img.shields.io/badge/PRIVILEGED_API-SHEVERY_MANAGER-f59e0b?style=flat-square) **Shevery Privileged API:** Installs `shevery-release.apk` (`com.shevery`) with full `WRITE_SECURE_SETTINGS`, `DUMP`, `PACKAGE_USAGE_STATS`, background `appops`, and Doze battery-whitelist exemptions.
* ![Ad Purge](https://img.shields.io/badge/AD_BLOCK-BOOT_%26_IDLE_PROMOS-ef4444?style=flat-square) **Boot Ad & Telemetry Purge:** Neutralizes `com.jio.stbadservice`, `com.jio.adc.aka`, `com.jio.adc.embedded`, `com.jio.bioscopecontentservice`, `com.jio.screensaver`, `com.jio.systemstats`, and `com.rjil.podlogservice`.
* ![Redownload Shield](https://img.shields.io/badge/OTA_SHIELD-AUTO--REDOWNLOAD_BLOCKER-8b5cf6?style=flat-square) **Silent App Re-Download & OTA Blocker:** Disables and unbinds `com.jio.httpupdate`, `com.jio.usbupdate`, `com.jio.fota`, `com.jio.media.jstb.fota`, `com.jio.stb.appstore`, `com.jio.stb.downloadmanager`, `com.rjil.jio.stb.commandreceiver`, `com.jio.backgroundscheduler`, and `com.iwedia.*` update daemons so removed applications (such as ETV) cannot silently reinstall themselves.

---

## Companion Utility: `ferrostb-toolbox.exe`

Packaged alongside `ferrostb.exe` inside `FerroSTB-Windows-Bundle.zip`, **FerroSTB Toolbox** is a self-contained CLI control center with embedded ADB binaries (`adb.exe`, `AdbWinApi.dll`, `AdbWinUsbApi.dll`) for ongoing device management:

| Option | Module | Description |
| :--- | :--- | :--- |
| **1** | **Wireless ADB Connect** | Auto-detects active STB sessions or connects to `<STB_IP>:5555`. |
| **2** | **APK Sideloader** | Drag-and-drop installer for single `.apk` files or batch directories of `.apk` packages. |
| **3** | **Safe-Mode App Uninstaller** | Lists user/ADB packages (`-3`) or preloaded partner apps while strictly filtering out core Android, SSO, and Bluetooth remote packages. |
| **4** | **System Package Restore** | Diffs firmware packages (`pm list packages -u`) against active packages and restores unbound/disabled system apps via `cmd package install-existing`. |
| **5** | **Launcher Switcher** | Queries all installed `CATEGORY_HOME` activities (or installs a new launcher `.apk`) and locks the selected launcher as default Home. |
| **6** | **tvOS Launcher Supercharger** | Installs or repairs the bundled `tvOS-Launcher.apk` and re-applies all privileged permissions and services. |
| **7** | **Home Button Fix** | Scans and neutralizes all stock `com.jio.*` and `com.iwedia.*` Home resolvers so the Home key stays bound to your custom launcher. |
| **8** | **Boot Ad & Promo Remover** | Purges startup video ads, ad injectors, and idle promotional screensavers. |
| **9** | **Auto-Redownload Blocker** | Neutralizes silent background app pushers and locks out unwanted partner apps (`etv`, `eenadu`, or custom package keywords). |
| **10** | **Performance & RAM Booster** | Adjusts window/transition/animator scales (`0.5x` or `0.0x`), trims package caches, and terminates idle background processes. |
| **11** | **Private DNS Switcher** | Configures Android 9+ DNS-over-TLS (`dns.adguard-dns.com`, `one.one.one.one`, `dns.google`, or Off). |
| **12** | **APK Extractor / Backup** | Pulls the installed `.apk` binary of any user application from the STB to `apk_backups\` on the host PC. |
| **13** | **Remote Helper & Screenshot** | Launches hidden Stock Android Settings or Developer Options on the TV, types text from the PC keyboard, or captures a 1080p PNG screenshot. |
| **14** | **Power & Reboot Menu** | Executes normal reboot, wireless ADB daemon restart (`tcpip 5555`), Recovery reboot, or Bootloader reboot. |

---

## System Architecture

| Module | Protocol / Port | Technical Implementation |
| :--- | :--- | :--- |
| **`src/assets.rs`** | In-Memory | Compile-time embedding (`include_bytes!`) of `adb.exe`, WinUSB DLLs, `acs_chain.pem`, `tvOS-Launcher.apk`, and `shevery-release.apk`. |
| **`src/main.rs`** | Win32 / Netsh | Evicts conflicting Windows UDP `:53` holders (`SharedAccess`, `icssvc`, `hns`), binds with `SO_REUSEADDR`, injects temporary Windows Firewall rules, and runs a parallel TCP `:5555` watcher. |
| **`src/dns.rs`** | `UDP :53` | Selective DNS spoofer routing `acs.oss.jio.com`, NTP, and connectivity checks to the host LAN IPv4 address while returning `NXDOMAIN` to unrelated traffic. |
| **`src/ntp.rs`** | `UDP :123` | RFC-compliant NTPv4 daemon synchronizing the STB's cold-boot clock (`1970`) to current UTC time so X.509 certificate and `DebugValidityDate` validations succeed. |
| **`src/captive.rs`** | `TCP :80` | Lightweight HTTP server returning `HTTP/1.1 204 No Content` while the router's WAN fiber is unplugged so Android marks the LAN connection as online. |
| **`src/cwmp.rs`** | `TCP :443, :7443, :8443, :9443` | Stateless OpenSSL (`DEFAULT:@SECLEVEL=0`) TR-069 ACS engine supporting `/common/v1/network/check` spoofing, dynamic SOAP namespace mirroring, shallow parameter discovery (`GetParameterNames`), and isolated single-parameter `SetParameterValues` RPCs. |
| **`src/adb.rs`** | `TCP :5555` | Automated ADB provisioner executing package installations, permission grants, stock launcher neutralization, and bloatware removal. |
| **`src/toolbox.rs`** | `TCP :5555` | Interactive post-provisioning CLI management suite (`ferrostb-toolbox.exe`). |

---

## Prerequisites

No external runtimes (Docker, Python, Node.js, or Technitium DNS) are required:

1. **Windows 10 or Windows 11 PC** connected to the same router as the Jio Set-Top Box.
2. **`FerroSTB-Windows-Bundle.zip`** downloaded from the **Releases** or **Actions** tab of this repository.
3. **Any empty USB Flash Drive** (used in Step 3 to open the native Android Storage Manager and access hidden network settings).

---

## Step-by-Step Provisioning Guide

### Step 1: Network Pairing

1. Connect your Windows PC to your primary home Wi-Fi (or Ethernet on the same router).
2. Connect your Jio STB to the same router.
3. Do not use a Guest Wi-Fi network (AP isolation prevents local device-to-device traffic).

---

### Step 2: Identify Your Windows Host IPv4 Address

1. Press `Win + R`, type `cmd`, and press **Enter**.
2. Execute:
   ```cmd
   ipconfig
   ```
3. Locate your active network adapter (**Wireless LAN adapter Wi-Fi** or **Ethernet**) and note the **IPv4 Address** (for example, `192.168.31.52`).

---

### Step 3: Point STB Static DNS to Your PC (USB Storage Breakout)

> [!NOTE]
> The stock Jio interface hides advanced DNS configuration. Inserting any USB thumb drive allows you to open Android's native Storage Manager and launch the underlying AOSP Settings application.

1. Insert any USB flash drive into a USB port on the Jio STB.
2. Power on the STB $\rightarrow$ Open **My Music** $\rightarrow$ Navigate to **Settings** $\rightarrow$ Select **Eject USB**.
3. Select **Internal Storage** $\rightarrow$ Open **Apps** $\rightarrow$ Select the system **Settings** application (gear icon) $\rightarrow$ Select **Open**.
4. Navigate to **Network & Internet** $\rightarrow$ Select your active Wi-Fi network $\rightarrow$ Open **IP Settings** (or **Advanced IP Settings**).
5. Note your STB's current DHCP **IP Address** (e.g., `192.168.31.181`) and **Gateway** (e.g., `192.168.31.1`).
6. Switch **IP Settings** from **DHCP** to **Static** and enter:
   * **IP Address:** Your STB's current LAN IP (overwrite the default `192.168.1.128` placeholder if cleared).
   * **Gateway:** Your router's LAN gateway IP (e.g., `192.168.31.1`).
   * **Network prefix length:** `24`
   * **DNS 1:** Your **Windows PC's IPv4 Address** from Step 2 (e.g., `192.168.31.52`).
   * **DNS 2:** Your **Windows PC's IPv4 Address** (e.g., `192.168.31.52`).
7. Save the configuration and confirm Wi-Fi reconnects.

---

### Step 4: Isolate Router WAN Fiber

1. Unplug the **incoming fiber-optic patch cable** (or WAN Ethernet cable) from your router so the STB cannot reach upstream servers.
2. **Keep the Wi-Fi router powered ON** so local LAN traffic flows between the STB and your PC.
3. If you use a third-party firewall, temporarily pause it (`ferrostb.exe` automatically adds temporary Windows Defender Firewall rules when run as Administrator).

---

### Step 5: Launch `ferrostb.exe` & Cold-Reboot the STB

1. Extract `FerroSTB-Windows-Bundle.zip`, right-click **`ferrostb.exe`**, and select **Run as administrator**.
2. Confirm all listeners report ready:
   ```text
   [READY] ALL SERVERS LISTENING!
   >>> Unplug the Jio STB power cord, wait 5s, and plug in.
   ```
3. **Cold Power-Cycle the STB:**
   * Unplug the power cable from the back of the Jio STB (do not use the remote standby button).
   * Wait **5 seconds**, then plug the power cable back in.
   * `ferrostb.exe` will automatically spoof DNS (`:53`), synchronize NTP time (`:123`), pass the Captive Portal check (`:80`), execute TR-069 CWMP unlocking (`:9443`), and initiate wireless ADB provisioning (`:5555`).

---

### Step 6: Authorize USB Debugging on the TV Screen

1. Once `ferrostb.exe` connects to port `5555`, an **"Allow USB Debugging?"** prompt will appear on your TV.
2. Check **"Always allow from this computer"** and select **OK / Allow** using the remote.
3. `ferrostb.exe` will finish installing **tvOS Launcher** and **Shevery**, lock the Home button, strip boot ads and OTA daemons, and launch directly into **tvOS Launcher**.

---

### Step 7: Restore WAN Connectivity

1. Reconnect the **fiber-optic / WAN cable** to your router.
2. On the Jio STB, switch Wi-Fi **IP Settings** from **Static** back to **DHCP**.
3. Run **`ferrostb-toolbox.exe`** on your PC whenever you want to sideload additional `.apk` files, switch launchers, block unwanted packages, or restore unbound system apps.

---

## Disclaimer

* **Scope:** This software modifies device configuration via TR-069 and ADB, including enabling wireless debugging, installing custom launchers and privileged management tools, and disabling/unbinding stock launcher, telemetry, OTA, and advertising packages.
* **Authorization:** Use strictly on hardware you personally own or are explicitly authorized to modify.