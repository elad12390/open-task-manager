//! `/proc` readers: the process table (parent, owner, memory, executable path), process state, and
//! the hardware and OS facts of the System Information panel.

use std::{collections::HashMap, ffi::CStr, fs, path::Path};

use crate::{metrics::PsRow, sysops::SystemInfo};

/// The fields of `/proc/<pid>/stat` after the command name, which may itself contain spaces.
fn stat_fields(pid: u32) -> Option<Vec<String>> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    Some(rest.split_whitespace().map(str::to_string).collect())
}

/// `/proc/<pid>/status` lines of the form `Key:\tvalue`.
fn status_value(pid: u32, key: &str) -> Option<String> {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status
        .lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))
        .map(|v| v.trim().to_string())
}

fn user_name(uid: u32) -> String {
    // SAFETY: getpwuid returns a pointer into static storage or null; the name is copied at once.
    unsafe {
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            return uid.to_string();
        }
        CStr::from_ptr((*pw).pw_name).to_string_lossy().into_owned()
    }
}

/// Parent, owner, resident memory and executable path of every process readable in `/proc`.
/// Processes whose files another user keeps private are skipped or come back with empty fields.
pub fn process_table() -> HashMap<u32, PsRow> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return HashMap::new();
    };
    entries
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| {
            let fields = stat_fields(pid)?;
            let ppid = fields.get(1)?.parse().ok()?;
            let uid = status_value(pid, "Uid")
                .and_then(|u| u.split_whitespace().next()?.parse().ok())
                .unwrap_or(u32::MAX);
            let memory = status_value(pid, "VmRSS")
                .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
                .map_or(0, |kb| kb * 1024);
            let path = fs::read_link(format!("/proc/{pid}/exe"))
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            Some((
                pid,
                PsRow {
                    ppid,
                    user: if uid == u32::MAX {
                        "unknown".into()
                    } else {
                        user_name(uid)
                    },
                    cpu: 0.,
                    memory,
                    path,
                },
            ))
        })
        .collect()
}

/// The kernel's state letter for a process ("R", "S", "T" = stopped…), or None if it is gone.
pub fn process_state(pid: u32) -> Option<String> {
    stat_fields(pid)?.first().cloned()
}

/// Kernel threads have no executable and are children of kthreadd (pid 2) or of pid 0.
pub fn is_kernel_thread(pid: u32, ppid: Option<u32>, path_is_empty: bool) -> bool {
    pid == 2 || ppid.is_some_and(|p| p == 0 || p == 2) && path_is_empty
}

fn cpu_model() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|info| {
            info.lines()
                .find_map(|l| l.strip_prefix("model name")?.strip_prefix(':'))
                .map(|m| m.trim().to_string())
        })
        .unwrap_or_default()
}

fn os_release(key: &str) -> String {
    fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|l| {
                l.strip_prefix(key)?
                    .strip_prefix('=')
                    .map(|v| v.trim_matches('"').to_string())
            })
        })
        .unwrap_or_default()
}

fn mem_total() -> u64 {
    fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|m| {
            m.lines().find_map(|l| {
                l.strip_prefix("MemTotal:")?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
        })
        .map_or(0, |kb| kb * 1024)
}

fn cpu_count(key: &str) -> u32 {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .map(|info| info.lines().filter(|l| l.starts_with(key)).count() as u32)
        .unwrap_or(0)
}

pub fn system_info() -> SystemInfo {
    let uptime: f64 = fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|u| u.split_whitespace().next()?.parse().ok())
        .unwrap_or(0.);
    let now = crate::sysops::now_secs();
    let release = fs::read_to_string(Path::new("/proc/sys/kernel/osrelease"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    SystemInfo {
        model: fs::read_to_string("/sys/devices/virtual/dmi/id/product_name")
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
        chip: cpu_model(),
        os: os_release("PRETTY_NAME"),
        build: release,
        arch: std::env::consts::ARCH.to_string(),
        memory: mem_total(),
        physical_cores: cpu_count("processor"),
        performance_cores: 0,
        efficiency_cores: 0,
        booted: now.saturating_sub(uptime as u64),
    }
}
