//! `/proc` and `/sys` readers: the process table (parent, owner, memory, executable path), process
//! state and cgroup, kernel threads, and the hardware and OS facts of the System Information panel.
//! Inside a sandbox the process facts come from the host's sample (`hostproc`), and every file is read
//! through `host`, so the same code reads the host's `/proc` and `/sys` in both cases.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    ffi::CStr,
    path::{Path, PathBuf},
};

use super::{host, hostproc};
use crate::{metrics::PsRow, sysops::SystemInfo};

/// The fields of `/proc/<pid>/stat` after the command name, which may itself contain spaces.
fn stat_fields(pid: u32) -> Option<Vec<String>> {
    let stat = host::read(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    Some(rest.split_whitespace().map(str::to_string).collect())
}

/// `/proc/<pid>/status` lines of the form `Key:\tvalue`.
fn status_value(pid: u32, key: &str) -> Option<String> {
    let status = host::read(format!("/proc/{pid}/status")).ok()?;
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

/// The executable: its `exe` link, else argv[0] when that is an absolute path (the link of another
/// user's process is often refused, but its command line is readable).
fn exe_path(pid: u32) -> String {
    if let Ok(p) = host::read_link(format!("/proc/{pid}/exe")) {
        return p.to_string_lossy().into_owned();
    }
    host::read_bytes(format!("/proc/{pid}/cmdline"))
        .ok()
        .and_then(|cmd| {
            let arg0 = String::from_utf8_lossy(cmd.split(|&b| b == 0).next()?).into_owned();
            arg0.starts_with('/').then_some(arg0)
        })
        .unwrap_or_default()
}

/// Parent, owner, resident memory and executable path of every process readable in `/proc`.
/// Processes whose files another user keeps private are skipped or come back with empty fields.
/// Inside a sandbox this is the host's table, from its latest sample.
pub fn process_table() -> HashMap<u32, PsRow> {
    if host::sandboxed() {
        return hostproc::sample()
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                (
                    p.pid,
                    PsRow {
                        ppid: p.ppid.unwrap_or(p.pid),
                        user: p.user,
                        cpu: p.cpu,
                        memory: p.memory,
                        path: p.path,
                    },
                )
            })
            .collect();
    }
    let Ok(entries) = host::read_dir("/proc") else {
        return HashMap::new();
    };
    entries
        .iter()
        .filter_map(|e| e.file_name()?.to_str()?.parse::<u32>().ok())
        .filter_map(|pid| {
            let fields = stat_fields(pid)?;
            let ppid = fields.get(1)?.parse().ok()?;
            let uid = status_value(pid, "Uid")
                .and_then(|u| u.split_whitespace().next()?.parse().ok())
                .unwrap_or(u32::MAX);
            let memory = status_value(pid, "VmRSS")
                .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
                .map_or(0, |kb| kb * 1024);
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
                    path: exe_path(pid),
                },
            ))
        })
        .collect()
}

/// The kernel's state letter for a process ("R", "S", "T" = stopped…), or None if it is gone.
pub fn process_state(pid: u32) -> Option<String> {
    if host::sandboxed() {
        let out = host::run(&["ps", "-o", "stat=", "-p", &pid.to_string()]).ok()?;
        return out.trim().chars().next().map(|c| c.to_string());
    }
    stat_fields(pid)?.first().cloned()
}

/// The cgroup lines of a process (`0::/user.slice/…` on cgroup v2), which name its systemd unit.
pub fn cgroup(pid: u32) -> Option<String> {
    if host::sandboxed() {
        return hostproc::cgroup(pid);
    }
    host::read(format!("/proc/{pid}/cgroup")).ok()
}

/// Kernel threads have no executable and are children of kthreadd (pid 2) or of pid 0.
pub fn is_kernel_thread(pid: u32, ppid: Option<u32>, path_is_empty: bool) -> bool {
    pid == 2 || ppid.is_some_and(|p| p == 0 || p == 2) && path_is_empty
}

/// A process the kernel runs itself: no command line and no executable, so nothing user-level to signal.
pub fn is_kernel_process(pid: u32) -> bool {
    if host::sandboxed() {
        return pid == 2 || hostproc::is_kernel(pid);
    }
    pid == 2
        || (pid > 1
            && host::read_bytes(format!("/proc/{pid}/cmdline")).is_ok_and(|c| c.is_empty())
            && host::read_link(format!("/proc/{pid}/exe")).is_err())
}

/// The app's own process ID as the process table names it (the host's, inside a sandbox).
pub fn own_pid() -> u32 {
    if host::sandboxed() {
        return hostproc::own_pid().unwrap_or(std::process::id());
    }
    std::process::id()
}

fn cpu_model() -> String {
    host::read("/proc/cpuinfo")
        .ok()
        .and_then(|info| {
            info.lines()
                .find_map(|l| l.strip_prefix("model name")?.strip_prefix(':'))
                .map(|m| m.trim().to_string())
        })
        .unwrap_or_default()
}

fn os_release(key: &str) -> String {
    host::read("/etc/os-release")
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
    host::read("/proc/meminfo")
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
    host::read("/proc/cpuinfo")
        .ok()
        .map(|info| info.lines().filter(|l| l.starts_with(key)).count() as u32)
        .unwrap_or(0)
}

/// One logical CPU: its id, its (package, core) pair and its capacity, where the kernel exposes them.
#[derive(Debug, PartialEq)]
struct Cpu {
    id: u32,
    core: Option<(u32, u32)>,
    capacity: Option<u32>,
}

fn read_u32(path: impl AsRef<Path>) -> Option<u32> {
    host::read(path).ok()?.trim().parse().ok()
}

fn cpus() -> Vec<Cpu> {
    let Ok(entries) = host::read_dir("/sys/devices/system/cpu") else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|dir| {
            let id: u32 = dir.file_name()?.to_str()?.strip_prefix("cpu")?.parse().ok()?;
            Some(Cpu {
                id,
                core: read_u32(dir.join("topology/physical_package_id"))
                    .zip(read_u32(dir.join("topology/core_id"))),
                capacity: read_u32(dir.join("cpu_capacity")),
            })
        })
        .collect()
}

/// A kernel CPU list such as `0-3,8,10-11`.
fn parse_cpulist(list: &str) -> Vec<u32> {
    list.trim()
        .split(',')
        .filter_map(|part| match part.split_once('-') {
            Some((a, b)) => Some((a.parse::<u32>().ok()?..=b.parse().ok()?).collect::<Vec<_>>()),
            None => part.parse().ok().map(|n| vec![n]),
        })
        .flatten()
        .collect()
}

/// (physical cores, performance cores, efficiency cores). P and E cores come from ARM's per-CPU
/// capacity, or from Intel hybrid's `cpu_core` and `cpu_atom` lists; without either they are 0.
fn count_cores(cpus: &[Cpu], hybrid: (&[u32], &[u32])) -> (u32, u32, u32) {
    let physical = unique_cores(cpus.iter());
    let capacities: BTreeSet<u32> = cpus.iter().filter_map(|c| c.capacity).collect();
    if let Some(&top) = capacities.last().filter(|_| capacities.len() >= 2) {
        let perf = unique_cores(cpus.iter().filter(|c| c.capacity == Some(top)));
        let eff = unique_cores(cpus.iter().filter(|c| c.capacity.is_some_and(|x| x < top)));
        return (physical, perf, eff);
    }
    let (p_list, e_list) = hybrid;
    if !p_list.is_empty() && !e_list.is_empty() {
        let perf = unique_cores(cpus.iter().filter(|c| p_list.contains(&c.id)));
        let eff = unique_cores(cpus.iter().filter(|c| e_list.contains(&c.id)));
        return (physical, perf, eff);
    }
    (physical, 0, 0)
}

/// How many distinct physical cores the CPUs belong to (SMT threads share one core).
fn unique_cores<'a>(cpus: impl Iterator<Item = &'a Cpu>) -> u32 {
    cpus.filter_map(|c| c.core).collect::<HashSet<_>>().len() as u32
}

fn cpu_list(path: &str) -> Vec<u32> {
    host::read(path)
        .map(|s| parse_cpulist(&s))
        .unwrap_or_default()
}

pub fn system_info() -> SystemInfo {
    let uptime: f64 = host::read("/proc/uptime")
        .ok()
        .and_then(|u| u.split_whitespace().next()?.parse().ok())
        .unwrap_or(0.);
    let now = crate::sysops::now_secs();
    let release = host::read("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let (physical, performance, efficiency) = count_cores(
        &cpus(),
        (
            &cpu_list("/sys/devices/cpu_core/cpus"),
            &cpu_list("/sys/devices/cpu_atom/cpus"),
        ),
    );
    SystemInfo {
        model: host::read("/sys/devices/virtual/dmi/id/product_name")
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
        chip: cpu_model(),
        os: os_release("PRETTY_NAME"),
        build: release,
        arch: std::env::consts::ARCH.to_string(),
        memory: mem_total(),
        physical_cores: if physical == 0 {
            cpu_count("processor")
        } else {
            physical
        },
        performance_cores: performance,
        efficiency_cores: efficiency,
        booted: now.saturating_sub(uptime as u64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpu(id: u32, core: (u32, u32), capacity: Option<u32>) -> Cpu {
        Cpu {
            id,
            core: Some(core),
            capacity,
        }
    }

    #[test]
    fn cpulists_expand_ranges() {
        assert_eq!(parse_cpulist("0-3,8,10-11\n"), vec![0, 1, 2, 3, 8, 10, 11]);
        assert_eq!(parse_cpulist("5"), vec![5]);
        assert!(parse_cpulist("").is_empty());
    }

    #[test]
    fn smt_threads_count_once_per_core() {
        let cpus = [
            cpu(0, (0, 0), None),
            cpu(1, (0, 0), None),
            cpu(2, (0, 1), None),
        ];
        assert_eq!(count_cores(&cpus, (&[], &[])), (2, 0, 0));
    }

    #[test]
    fn arm_capacity_splits_performance_and_efficiency() {
        let cpus = [
            cpu(0, (0, 0), Some(446)),
            cpu(1, (0, 1), Some(446)),
            cpu(2, (0, 2), Some(1024)),
            cpu(3, (0, 3), Some(1024)),
            cpu(4, (0, 4), Some(1024)),
        ];
        assert_eq!(count_cores(&cpus, (&[], &[])), (5, 3, 2));
    }

    #[test]
    fn intel_hybrid_lists_split_the_cores() {
        let cpus = [
            cpu(0, (0, 0), None),
            cpu(1, (0, 0), None),
            cpu(2, (0, 1), None),
            cpu(16, (0, 16), None),
        ];
        assert_eq!(count_cores(&cpus, (&[0, 1, 2], &[16])), (3, 2, 1));
    }
}
