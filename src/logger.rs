use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::task::JoinHandle;

#[derive(Clone, Debug, Serialize)]
pub struct LogEntry {
    pub id: u64,
    pub ts: String,
    pub stage: String,
    pub level: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticReport {
    pub stage: String,
    pub title: String,
    pub reason: String,
    pub fixes: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProvisionSnapshot {
    pub status: String, // idle | running | success | failed
    pub family: String, // jhs | jms
    pub host_ip: String,
    pub jhs_cert_ready: bool,
    pub jhs_cert_source: String,
    pub active_target: Option<String>,
    pub detected_stb_ip: Option<String>,
    pub dns_bound: bool,
    pub ntp_bound: bool,
    pub http_bound: bool,
    pub cwmp_bound: bool,
    pub dns_spoof_count: u64,
    pub ntp_synced: bool,
    pub captive_passed: bool,
    pub cwmp_connected: bool,
    pub cwmp_set_ok: bool,
    pub port5555_open: bool,
    pub adb_authorized: bool,
    pub tls_rejection: Option<String>,
    pub current_step: u8, // 0..=5
    pub step_label: String,
    pub failure: Option<DiagnosticReport>,
    pub success_summary: Vec<String>,
    pub logs: Vec<LogEntry>,
}

pub struct InnerState {
    pub status: String,
    pub family: String,
    pub host_ip: String,
    pub jhs_cert_ready: bool,
    pub jhs_cert_source: String,
    pub active_target: Option<String>,
    pub detected_stb_ip: Option<String>,
    pub dns_bound: bool,
    pub ntp_bound: bool,
    pub http_bound: bool,
    pub cwmp_bound: bool,
    pub dns_spoof_count: u64,
    pub ntp_synced: bool,
    pub captive_passed: bool,
    pub cwmp_connected: bool,
    pub cwmp_set_ok: bool,
    pub port5555_open: bool,
    pub adb_authorized: bool,
    pub tls_rejection: Option<String>,
    pub current_step: u8,
    pub step_label: String,
    pub failure: Option<DiagnosticReport>,
    pub success_summary: Vec<String>,
    pub logs: VecDeque<LogEntry>,
    pub next_log_id: u64,
    pub bg_tasks: Vec<JoinHandle<()>>,
}

pub struct AppState {
    pub inner: Mutex<InnerState>,
    pub cancel_flag: AtomicBool,
}

impl AppState {
    pub fn new(host_ip: String, jhs_cert_ready: bool, jhs_cert_source: String, active_target: Option<String>) -> Self {
        Self {
            inner: Mutex::new(InnerState {
                status: "idle".to_string(),
                family: "jhs".to_string(),
                host_ip,
                jhs_cert_ready,
                jhs_cert_source,
                active_target,
                detected_stb_ip: None,
                dns_bound: false,
                ntp_bound: false,
                http_bound: false,
                cwmp_bound: false,
                dns_spoof_count: 0,
                ntp_synced: false,
                captive_passed: false,
                cwmp_connected: false,
                cwmp_set_ok: false,
                port5555_open: false,
                adb_authorized: false,
                tls_rejection: None,
                current_step: 0,
                step_label: "Ready to start".to_string(),
                failure: None,
                success_summary: Vec::new(),
                logs: VecDeque::with_capacity(400),
                next_log_id: 1,
                bg_tasks: Vec::new(),
            }),
            cancel_flag: AtomicBool::new(false),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }

    pub fn log(&self, stage: &str, level: &str, msg: impl Into<String>) {
        let message = msg.into();
        println!("[{}:{}] {}", stage, level.to_uppercase(), message);

        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let hh = (secs / 3600) % 24;
        let mm = (secs / 60) % 60;
        let ss = secs % 60;
        let ts = format!("{:02}:{:02}:{:02}", hh, mm, ss);

        if let Ok(mut st) = self.inner.lock() {
            let id = st.next_log_id;
            st.next_log_id += 1;
            if st.logs.len() >= 350 {
                st.logs.pop_front();
            }
            st.logs.push_back(LogEntry {
                id,
                ts,
                stage: stage.to_string(),
                level: level.to_string(),
                message,
            });
        }
    }

    pub fn reset_for_provision(&self, family: &str) {
        self.cancel_flag.store(false, Ordering::SeqCst);
        if let Ok(mut st) = self.inner.lock() {
            for h in st.bg_tasks.drain(..) {
                h.abort();
            }
            st.status = "running".to_string();
            st.family = family.to_string();
            st.detected_stb_ip = None;
            st.dns_bound = false;
            st.ntp_bound = false;
            st.http_bound = false;
            st.cwmp_bound = false;
            st.dns_spoof_count = 0;
            st.ntp_synced = false;
            st.captive_passed = false;
            st.cwmp_connected = false;
            st.cwmp_set_ok = false;
            st.port5555_open = false;
            st.adb_authorized = false;
            st.tls_rejection = None;
            st.current_step = 1;
            st.step_label = "Starting spoofing stack...".to_string();
            st.failure = None;
            st.success_summary.clear();
            st.logs.clear();
        }
    }

    pub fn stop_provision(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        if let Ok(mut st) = self.inner.lock() {
            for h in st.bg_tasks.drain(..) {
                h.abort();
            }
            if st.status == "running" {
                st.status = "idle".to_string();
                st.current_step = 0;
                st.step_label = "Stopped by user".to_string();
            }
            st.dns_bound = false;
            st.ntp_bound = false;
            st.http_bound = false;
            st.cwmp_bound = false;
        }
    }

    pub fn register_task(&self, handle: JoinHandle<()>) {
        if let Ok(mut st) = self.inner.lock() {
            st.bg_tasks.push(handle);
        }
    }

    pub fn abort_servers(&self) {
        if let Ok(mut st) = self.inner.lock() {
            for h in st.bg_tasks.drain(..) {
                h.abort();
            }
            st.dns_bound = false;
            st.ntp_bound = false;
            st.http_bound = false;
            st.cwmp_bound = false;
        }
    }

    pub fn set_step(&self, step: u8, label: &str) {
        if let Ok(mut st) = self.inner.lock() {
            if step >= st.current_step {
                st.current_step = step;
                st.step_label = label.to_string();
            }
        }
    }

    pub fn set_failure(&self, stage: &str, title: &str, reason: impl Into<String>, fixes: Vec<&str>) {
        let reason_str = reason.into();
        self.log(stage, "error", format!("{}: {}", title, reason_str));
        if let Ok(mut st) = self.inner.lock() {
            st.status = "failed".to_string();
            st.step_label = format!("Failed at {}", stage);
            st.failure = Some(DiagnosticReport {
                stage: stage.to_string(),
                title: title.to_string(),
                reason: reason_str,
                fixes: fixes.into_iter().map(|s| s.to_string()).collect(),
            });
        }
    }

    pub fn set_success(&self, target: String, summary: Vec<String>) {
        if let Ok(mut st) = self.inner.lock() {
            st.status = "success".to_string();
            st.current_step = 5;
            st.step_label = "Provisioning Complete!".to_string();
            st.active_target = Some(target);
            st.success_summary = summary;
            st.failure = None;
        }
    }

    pub fn set_active_target(&self, target: Option<String>) {
        if let Ok(mut st) = self.inner.lock() {
            st.active_target = target;
        }
    }

    pub fn active_target(&self) -> Option<String> {
        self.inner.lock().ok().and_then(|st| st.active_target.clone())
    }

    pub fn set_host_ip(&self, ip: String) {
        if let Ok(mut st) = self.inner.lock() {
            st.host_ip = ip;
        }
    }

    pub fn host_ip(&self) -> String {
        self.inner
            .lock()
            .map(|st| st.host_ip.clone())
            .unwrap_or_else(|_| "Unknown".to_string())
    }

    pub fn snapshot(&self) -> ProvisionSnapshot {
        let st = self.inner.lock().unwrap();
        ProvisionSnapshot {
            status: st.status.clone(),
            family: st.family.clone(),
            host_ip: st.host_ip.clone(),
            jhs_cert_ready: st.jhs_cert_ready,
            jhs_cert_source: st.jhs_cert_source.clone(),
            active_target: st.active_target.clone(),
            detected_stb_ip: st.detected_stb_ip.clone(),
            dns_bound: st.dns_bound,
            ntp_bound: st.ntp_bound,
            http_bound: st.http_bound,
            cwmp_bound: st.cwmp_bound,
            dns_spoof_count: st.dns_spoof_count,
            ntp_synced: st.ntp_synced,
            captive_passed: st.captive_passed,
            cwmp_connected: st.cwmp_connected,
            cwmp_set_ok: st.cwmp_set_ok,
            port5555_open: st.port5555_open,
            adb_authorized: st.adb_authorized,
            tls_rejection: st.tls_rejection.clone(),
            current_step: st.current_step,
            step_label: st.step_label.clone(),
            failure: st.failure.clone(),
            success_summary: st.success_summary.clone(),
            logs: st.logs.iter().cloned().collect(),
        }
    }
}
