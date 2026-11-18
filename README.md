# FerroSTB & FerroSTB Toolbox

**Zero-Dependency Native Rust TR-069 Provisioner, Ad-Blocker & Post-Unlock Management Suite for Jio Set-Top Boxes (Windows & Linux)**

## Overview

**FerroSTB** is a standalone, ground-up Rust provisioning engine (`ferrostb` / `ferrostb.exe`) paired with an interactive post-unlock management utility (`ferrostb-toolbox` / `ferrostb-toolbox.exe`). It eliminates legacy multi-container stacks (Docker, MongoDB, Node.js, Python proxies, and external DNS daemons) by compiling the entire network spoofing, TR-069 CWMP TLS server, and ADB provisioning pipeline into native Windows and Linux executables with all runtime payloads embedded directly in memory.

> \[!NOTE\]
> **Supported Hardware & Firmware**
>
> * **JMSC200Av2** and **JMSB200A** across firmware builds `22.1.09`, `24.1.39`, `25.1.9`, and earlier.
>
> * **JHS Series (JHSC200 v1 / JioOS 2.0)** on firmware build `stb-jhcs200-11.3.46`.

> \[!TIP\]
> **System Recovery & Safety**
>
> * **Accidental App Removal:** Launch `ferrostb-toolbox` and select **Option 4 (Restore System Packages)** to re-bind any uninstalled or disabled firmware package (`cmd package install-existing`) without factory resetting the device.
>
> * **Full Reset:** Performing a standard factory reset restores the Set-Top Box to its stock out-of-box state at any time.
>
> * **Wi-Fi Reconnection:** If Wi-Fi fails to authenticate after switching IP modes, select **Forget Network**, re-enter your Wi-Fi password, and reconnect.

> \[!IMPORTANT\]
> **Sideloading APK Files**
> Because the stock Android package installer UI is disabled in firmware, clicking an `.apk` file inside a TV file manager will not install it. Use **`ferrostb-toolbox` (Option 2)** on your Windows or Linux host machine to drag-and-drop and install `.apk` packages over wireless ADB.

## Core Provisioning Capabilities (`ferrostb`)

* **Persistent Wireless ADB:** Enables network ADB debugging over TCP port `5555` (validity set through `2037`), runs a parallel TCP `:5555` detector, and persists global ADB flags across reboots.

* **tvOS Launcher Integration:** Sideloads `tvOS-Launcher.apk` (`com.techgenius.tvoslauncher`) and automatically grants `WRITE_SECURE_SETTINGS`, `PACKAGE_USAGE_STATS`, `SYSTEM_ALERT_WINDOW`, `ACCESS_FINE_LOCATION`, Notification Listener (`MediaWatchService`), Accessibility Service (`PowerService`), and Device Admin (`SleepReceiver$Admin`).

* **Permanent Home Button Lock:** Force-stops, clears, suspends, disables, and unbinds `com.jio.stb.screens` alongside Reliance's high-priority `com.iwedia.ui.rjio.launcher.reliance` home activity so pressing the remote's **Home** button never reverts to the stock interface.

* **Shevery Privileged API:** Installs `shevery-release.apk` (`com.shevery`) with full `WRITE_SECURE_SETTINGS`, `DUMP`, `PACKAGE_USAGE_STATS`, background `appops`, and Doze battery-whitelist exemptions.

* **Boot Ad & Telemetry Purge:** Neutralizes `com.jio.stbadservice`, `com.jio.adc.aka`, `com.jio.adc.embedded`, `com.jio.bioscopecontentservice`, `com.jio.screensaver`, `com.jio.systemstats`, and `com.rjil.podlogservice`.

* **Silent App Re-Download & OTA Blocker:** Disables and unbinds `com.jio.httpupdate`, `com.jio.usbupdate`, `com.jio.fota`, `com.jio.media.jstb.fota`, `com.jio.stb.appstore`, `com.jio.stb.downloadmanager`, `com.rjil.jio.stb.commandreceiver`, `com.jio.backgroundscheduler`, and `com.iwedia.*` update daemons so removed applications (such as ETV) cannot silently reinstall themselves.

* **Cross-Platform Port & Firewall Automation:** Automatically frees UDP `:53` and UDP `:123` from conflicting system daemons (`SharedAccess` / `hns` / `w32time` on Windows; `systemd-resolved` / `systemd-timesyncd` on Linux) and configures temporary firewall rules on startup, restoring host services cleanly on exit.

## Companion Utility: `ferrostb-toolbox`

Packaged alongside `ferrostb` inside every Windows and Linux release bundle, **FerroSTB Toolbox** is a self-contained CLI control center with embedded ADB binaries (`adb.exe` + WinUSB DLLs on Windows; native `adb` binary on Linux) for ongoing device management:

| Option | Module | Description | 
| ----- | ----- | ----- | 
| **1** | **Wireless ADB Connect** | Auto-detects active STB sessions or connects to `<STB_IP>:5555`. | 
| **2** | **APK Sideloader** | Drag-and-drop installer for single `.apk` files or batch directories of `.apk` packages. | 
| **3** | **Safe-Mode App Uninstaller** | Lists user/ADB packages (`-3`) or preloaded partner apps while strictly filtering out core Android, SSO, and Bluetooth remote packages. | 
| **4** | **System Package Restore** | Diffs firmware packages (`pm list packages -u`) against active packages and restores unbound/disabled system apps via `cmd package install-existing`. | 
| **5** | **Launcher Switcher** | Queries all installed `CATEGORY_HOME` activities (or installs a new launcher `.apk`) and locks the selected launcher as default Home. | 
| **6** | **tvOS Launcher Supercharger** | Installs or repairs the bundled `tvOS-Launcher.apk` and re-applies all privileged permissions and services. | 
| **7** | **Home Button Fix** | Scans and neutralizes all stock `com.jio.*` and `com.iwedia.*` Home resolvers so the Home key stays bound to your custom launcher. | 
| **8** | **Boot Ad & Promo Remover** | Purges startup video ads, ad injectors, and idle promotional screensavers. | 
| **9** | **Auto-Redownload Blocker** | Neutralizes silent background app pushers and locks out unwanted partner apps (`etv`, `eenadu`, or custom package keywords). | 
| **10** | **Performance & RAM Booster** | Adjusts window/transition/animator scales (`0.5x` or `0.0x`), trims package caches, and frees background RAM. | 
| **11** | **Private DNS Switcher** | Configures Android 9+ DNS-over-TLS (`dns.adguard-dns.com`, `one.one.one.one`, `dns.google`, or Off). | 
| **12** | **APK Extractor / Backup** | Pulls the installed `.apk` binary of any user application from the STB to `apk_backups/` on the host machine. | 
| **13** | **Remote Helper & Screenshot** | Launches hidden Stock Android Settings or Developer Options on the TV, types text from the host keyboard, or captures a 1080p PNG screenshot. | 
| **14** | **Power & Reboot Menu** | Executes normal reboot, wireless ADB daemon restart (`tcpip 5555`), Recovery reboot, or Bootloader reboot. | 

## System Architecture

| Module | Protocol / Port | Technical Implementation | 
| ----- | ----- | ----- | 
| **`src/assets.rs`** | In-Memory | Compile-time embedding (`include_bytes!`) of platform-native `adb` binaries, `acs_chain.pem`, `tvOS-Launcher.apk`, and `shevery-release.apk`. | 
| **`src/main.rs`** | Win32 / POSIX | Evicts conflicting UDP `:53` / `:123` services (`SharedAccess` on Windows; `systemd-resolved` on Linux), binds with `SO_REUSEADDR`, applies firewall rules, and runs a parallel TCP `:5555` watcher. | 
| **`src/dns.rs`** | `UDP :53` | Selective DNS spoofer routing `acs.oss.jio.com`, NTP, and connectivity checks to the host LAN IPv4 address while returning `NXDOMAIN` to unrelated traffic. | 
| **`src/ntp.rs`** | `UDP :123` | RFC-compliant NTPv4 daemon synchronizing the STB's cold-boot clock (`1970`) to current UTC time so X.509 certificate and `DebugValidityDate` validations succeed. | 
| **`src/captive.rs`** | `TCP :80` | Lightweight HTTP server returning `HTTP/1.1 204 No Content` while the router's WAN fiber is unplugged so Android marks the LAN connection as online. | 
| **`src/cwmp.rs`** | `TCP :443, :7443, :8443, :9443` | Stateless OpenSSL (`DEFAULT:@SECLEVEL=0`) TR-069 ACS engine supporting `/common/v1/network/check` spoofing, dynamic SOAP namespace mirroring, shallow parameter discovery (`GetParameterNames`), and isolated single-parameter `SetParameterValues` RPCs. | 
| **`src/adb.rs`** | `TCP :5555` | Automated ADB provisioner executing package installations, permission grants, stock launcher neutralization, and bloatware removal. | 
| **`src/toolbox.rs`** | `TCP :5555` | Interactive post-provisioning CLI management suite (`ferrostb-toolbox`). | 

## Prerequisites

No external runtimes (Docker, Python, Node.js, or Technitium DNS) are required:

1. **Host Computer:** A **Windows 10 / 11 PC** or **Linux (`x86_64`)** machine connected to the same router as your Jio Set-Top Box.

2. **Release Bundle:** Download the latest bundle from the [**Releases**](../../releases) tab:

   * **Windows:** `FerroSTB-Windows-Bundle.zip` (contains `ferrostb.exe` and `ferrostb-toolbox.exe`)

   * **Linux:** `FerroSTB-Linux-Bundle.zip` or `FerroSTB-Linux-Bundle.tar.gz` (contains `ferrostb` and `ferrostb-toolbox`)

3. **USB Flash Drive:** Any empty USB thumb drive (used briefly in Step 3 to open the native Android Storage Manager and access hidden network settings).

## Step-by-Step Provisioning Guide

### Step 1: Network Pairing

1. Connect your Windows or Linux host machine to your primary home Wi-Fi (or Ethernet on the same router).

2. Connect your Jio STB to the same router.

3. Do not use a Guest Wi-Fi network (AP isolation prevents local device-to-device traffic).

### Step 2: Identify Your Host IPv4 Address

* **On Windows:**

  1. Press `Win + R`, type `cmd`, and press **Enter**.

  2. Run `ipconfig` and note the **IPv4 Address** under your active Wi-Fi or Ethernet adapter (for example, `192.168.31.52`).

* **On Linux:**

  1. Open a terminal and run:

     ```
     ip -4 addr show scope global
     
     ```

  2. Note the `inet` IPv4 address on your active LAN/Wi-Fi interface (for example, `192.168.31.52`).

### Step 3: Point STB Static DNS to Your Host (USB Storage Breakout)

> \[!NOTE\]
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

   * **DNS 1:** Your **Host PC's IPv4 Address** from Step 2 (e.g., `192.168.31.52`).

   * **DNS 2:** Your **Host PC's IPv4 Address** (e.g., `192.168.31.52`).

7. Save the configuration and confirm Wi-Fi reconnects.

### Step 4: Isolate Router WAN Fiber

1. Unplug the **incoming fiber-optic patch cable** (or WAN Ethernet cable) from your router so the STB cannot reach upstream servers.

2. **Keep the Wi-Fi router powered ON** so local LAN traffic flows between the STB and your host machine.

3. If you run a strict third-party firewall (`firewalld`, `iptables`, or Windows security suites), temporarily allow inbound traffic on UDP `53, 123` and TCP `80, 443, 7443, 8443, 9443`.

### Step 5: Launch `ferrostb` & Cold-Reboot the STB

* **On Windows:**
  Extract `FerroSTB-Windows-Bundle.zip`, right-click **`ferrostb.exe`**, and select **Run as administrator**.

* **On Linux:**
  Extract `FerroSTB-Linux-Bundle.tar.gz` (or `.zip`), make the binaries executable, and run with `sudo` (required to bind privileged ports `53`, `80`, `123`, and `443`):

  ```
  tar -xzf FerroSTB-Linux-Bundle.tar.gz
  cd FerroSTB-Linux-Bundle
  chmod +x ferrostb ferrostb-toolbox
  sudo ./ferrostb
  
  ```

Once all listeners report ready:

```
[READY] ALL SERVERS LISTENING!
>>> Unplug the Jio STB power cord, wait 5s, and plug in.

```

**Cold Power-Cycle the STB:**

1. Unplug the power cable from the back of the Jio STB (do not use the remote standby button).

2. Wait **5 seconds**, then plug the power cable back in.

3. `ferrostb` will automatically spoof DNS (`:53`), synchronize NTP time (`:123`), pass the Captive Portal check (`:80`), execute TR-069 CWMP unlocking (`:9443`), and initiate wireless ADB provisioning (`:5555`).

### Step 6: Authorize USB Debugging on the TV Screen

1. Once `ferrostb` connects to port `5555`, an **"Allow USB Debugging?"** prompt will appear on your TV.

2. Check **"Always allow from this computer"** and select **OK / Allow** using the remote.

3. `ferrostb` will finish installing **tvOS Launcher** and **Shevery**, lock the Home button, strip boot ads and OTA daemons, and launch directly into **tvOS Launcher**.

### Step 7: Restore WAN Connectivity & Manage with `ferrostb-toolbox`

1. Reconnect the **fiber-optic / WAN cable** to your router.

2. On the Jio STB, switch Wi-Fi **IP Settings** from **Static** back to **DHCP**.

3. Run **`ferrostb-toolbox.exe`** (Windows) or **`./ferrostb-toolbox`** (Linux) whenever you want to sideload additional `.apk` files, switch launchers, block unwanted packages, or restore unbound system apps.

## Disclaimer

* **Scope:** This software modifies device configuration via TR-069 and ADB, including enabling wireless debugging, installing custom launchers and privileged management tools, and disabling/unbinding stock launcher, telemetry, OTA, and advertising packages.

* **Authorization:** Use strictly on hardware you personally own or are explicitly authorized to modify.