//! System queries and guarded actions shared by every platform: the data folder, disk-scan history,
//! benchmarks and signals. The OS-specific queries live in `crate::platform` and are re-exported
//! here, so callers keep using `sysops::…`.

use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::platform::{bypass_cache, data_root};
pub use crate::platform::{
    connections, control_service, installed_apps, power, process_state, reveal, services,
    startup_items, system_info, toggle_startup, trash_app,
};

pub(crate) fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// Where settings, layouts, scans and recordings live. `OTM_DATA_DIR` overrides it (used by tests).
pub fn data_dir() -> PathBuf {
    std::env::var_os("OTM_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(data_root)
}

pub(crate) fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() {
            format!("{cmd} exited with {}", out.status)
        } else {
            err
        })
    }
}

// ---------- system info ----------

#[derive(Clone, Debug, Default)]
pub struct SystemInfo {
    pub model: String,
    pub chip: String,
    pub os: String,
    pub build: String,
    pub arch: String,
    pub memory: u64,
    pub physical_cores: u32,
    pub performance_cores: u32,
    pub efficiency_cores: u32,
    pub booted: u64,
}

// ---------- services and startup items ----------

#[derive(Clone, Debug)]
pub struct Service {
    pub label: String,
    pub pid: Option<u32>,
    pub status: i32,
    /// The program of a service that is not running, from its launch agent or daemon plist.
    pub program: Option<PathBuf>,
    /// "System" or "User" on Linux; empty where a platform has a single scope.
    pub scope: &'static str,
}

#[derive(Clone, Debug)]
pub struct StartupItem {
    pub label: String,
    pub scope: &'static str,
    pub file: PathBuf,
    pub loaded: bool,
    pub editable: bool,
    /// The program the launch agent or daemon runs.
    pub program: Option<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    Restart,
    Stop,
}

// ---------- processes ----------

pub fn signal_process(pid: u32, force: bool) -> Result<(), String> {
    send_signal(pid, if force { libc::SIGKILL } else { libc::SIGTERM })
}

/// Send any signal (TERM, KILL, STOP to suspend, CONT to resume). launchd and the kernel are refused.
pub fn send_signal(pid: u32, sig: i32) -> Result<(), String> {
    if pid <= 1 || crate::platform::is_kernel_process(pid) {
        return Err("Can't signal launchd or the kernel".into());
    }
    if unsafe { libc::kill(pid as i32, sig) } == 0 {
        Ok(())
    } else {
        Err(format!("PID {pid}: {}", std::io::Error::last_os_error()))
    }
}

// ---------- network ----------

#[derive(Clone, Debug)]
pub struct Connection {
    pub pid: u32,
    pub process: String,
    pub proto: String,
    pub endpoint: String,
    pub state: String,
}

// ---------- installed apps ----------

#[derive(Clone, Debug)]
pub struct InstalledApp {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub bytes: Option<u64>,
    /// Where the app came from, as the Source column shows it.
    pub source: &'static str,
}

/// The size of each app: the size the platform already read, else `du` of its path.
pub fn app_sizes(apps: &[InstalledApp]) -> Vec<Option<u64>> {
    apps.iter()
        .map(|a| {
            a.bytes.or_else(|| {
                run("du", &["-sk", &a.path.to_string_lossy()])
                    .ok()
                    .and_then(|o| o.split_whitespace().next()?.parse::<u64>().ok())
                    .map(|k| k * 1024)
            })
        })
        .collect()
}

// ---------- power ----------

#[derive(Clone, Debug, Default)]
pub struct Power {
    pub has_battery: bool,
    pub source: String,
    pub percent: Option<u32>,
    pub status: String,
    pub watts: Option<f64>,
    pub voltage: Option<f64>,
    pub cycles: Option<i64>,
    pub health: Option<f64>,
    pub temperature: Option<f64>,
    pub low_power_mode: bool,
    pub thermal: String,
}

// ---------- disk space ----------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FolderSize {
    pub path: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scan {
    pub at: u64,
    pub items: Vec<FolderSize>,
}

fn scans_file() -> PathBuf {
    data_dir().join("disk-scans.json")
}

pub fn load_scans() -> Vec<Scan> {
    fs::read(scans_file())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Size every top-level item in the home folder (capped at 5 minutes) and append to the saved history.
pub fn scan_home() -> Vec<Scan> {
    let home = home();
    let mut paths: Vec<String> = fs::read_dir(&home)
        .map(|r| {
            r.flatten()
                .map(|e| e.path().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    paths.retain(|p| !p.ends_with("/Library/Mobile Documents"));
    let mut args = vec!["300", "du", "-xsk"];
    args.extend(paths.iter().map(String::as_str));
    let out = Command::new("timeout")
        .args(&args)
        .output()
        .or_else(|_| Command::new("du").args(&args[2..]).output())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let mut items: Vec<FolderSize> = out
        .lines()
        .filter_map(|l| {
            let (k, p) = l.split_once('\t')?;
            Some(FolderSize {
                path: p.to_string(),
                bytes: k.trim().parse::<u64>().ok()? * 1024,
            })
        })
        .collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    items.truncate(30);
    let mut scans = load_scans();
    scans.insert(
        0,
        Scan {
            at: now_secs(),
            items,
        },
    );
    scans.truncate(10);
    let _ = fs::create_dir_all(data_dir());
    if let Ok(json) = serde_json::to_vec(&scans) {
        let _ = fs::write(scans_file(), json);
    }
    scans
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------- benchmarks ----------

#[derive(Clone, Debug, Default)]
pub struct BenchResult {
    pub single_core: f64,
    pub multi_core: f64,
    pub threads: usize,
    pub memory_gbps: f64,
    pub disk_write_mbps: f64,
    pub disk_read_mbps: f64,
}

/// Integer mixing workload (xorshift + multiply) per thread; score is millions of rounds per second.
fn cpu_work(rounds: u64) -> u64 {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut acc = 0u64;
    for i in 0..rounds {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        acc = acc.wrapping_add(x.wrapping_mul(i | 1)).rotate_left(5);
    }
    acc
}

fn cpu_score(threads: usize) -> f64 {
    const ROUNDS: u64 = 200_000_000;
    let start = Instant::now();
    let handles: Vec<_> = (0..threads)
        .map(|_| std::thread::spawn(|| cpu_work(ROUNDS)))
        .collect();
    let mut sink = 0u64;
    for h in handles {
        sink ^= h.join().unwrap_or(0);
    }
    std::hint::black_box(sink);
    (ROUNDS as f64 * threads as f64) / start.elapsed().as_secs_f64() / 1e6
}

fn memory_bandwidth() -> f64 {
    let len = 256 * 1024 * 1024 / 8;
    let src = vec![1.5f64; len];
    let mut dst = vec![0f64; len];
    let start = Instant::now();
    for _ in 0..4 {
        dst.copy_from_slice(&src);
        std::hint::black_box(&dst);
    }
    (len * 8 * 2 * 4) as f64 / start.elapsed().as_secs_f64() / 1e9
}

/// Sequential 512 MB write and read with the page cache bypassed (see `platform::bypass_cache`).
fn disk_speed() -> (f64, f64) {
    const SIZE: usize = 512 * 1024 * 1024;
    const CHUNK: usize = 8 * 1024 * 1024;
    let path = std::env::temp_dir().join(format!("otm-bench-{}.bin", std::process::id()));
    let block = vec![0xA5u8; CHUNK];
    let write = (|| -> std::io::Result<f64> {
        let mut f = fs::File::create(&path)?;
        bypass_cache(&f);
        let start = Instant::now();
        for _ in 0..SIZE / CHUNK {
            f.write_all(&block)?;
        }
        f.sync_all()?;
        Ok(SIZE as f64 / start.elapsed().as_secs_f64() / 1e6)
    })();
    let read = (|| -> std::io::Result<f64> {
        let mut f = fs::File::open(&path)?;
        bypass_cache(&f);
        let mut buf = vec![0u8; CHUNK];
        let start = Instant::now();
        while f.read(&mut buf)? > 0 {}
        Ok(SIZE as f64 / start.elapsed().as_secs_f64() / 1e6)
    })();
    let _ = fs::remove_file(&path);
    (write.unwrap_or(0.), read.unwrap_or(0.))
}

pub fn run_benchmarks() -> BenchResult {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let single_core = cpu_score(1);
    let multi_core = cpu_score(threads);
    let memory_gbps = memory_bandwidth();
    let (disk_write_mbps, disk_read_mbps) = disk_speed();
    BenchResult {
        single_core,
        multi_core,
        threads,
        memory_gbps,
        disk_write_mbps,
        disk_read_mbps,
    }
}
