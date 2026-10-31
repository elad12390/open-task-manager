//! macOS system queries and guarded actions (launchd, lsof, Finder, pmset, du, benchmarks).

use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    os::unix::io::AsRawFd,
    path::{Path, PathBuf},
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// Where settings, layouts, scans and recordings live. `OTM_DATA_DIR` overrides it (used by tests).
pub fn data_dir() -> PathBuf {
    std::env::var_os("OTM_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("Library/Application Support/OpenTaskManager"))
}

fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
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

fn uid() -> u32 {
    unsafe { libc::getuid() }
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

fn sysctl(key: &str) -> String {
    run("sysctl", &["-n", key])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn system_info() -> SystemInfo {
    let booted = sysctl("kern.boottime")
        .split("sec = ")
        .nth(1)
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    SystemInfo {
        model: sysctl("hw.model"),
        chip: sysctl("machdep.cpu.brand_string"),
        os: format!(
            "{} {}",
            run("sw_vers", &["-productName"]).unwrap_or_default().trim(),
            run("sw_vers", &["-productVersion"])
                .unwrap_or_default()
                .trim()
        ),
        build: run("sw_vers", &["-buildVersion"])
            .unwrap_or_default()
            .trim()
            .to_string(),
        arch: run("uname", &["-m"]).unwrap_or_default().trim().to_string(),
        memory: sysctl("hw.memsize").parse().unwrap_or(0),
        physical_cores: sysctl("hw.physicalcpu").parse().unwrap_or(0),
        performance_cores: sysctl("hw.perflevel0.physicalcpu").parse().unwrap_or(0),
        efficiency_cores: sysctl("hw.perflevel1.physicalcpu").parse().unwrap_or(0),
        booted,
    }
}

// ---------- launchd ----------

#[derive(Clone, Debug)]
pub struct Service {
    pub label: String,
    pub pid: Option<u32>,
    pub status: i32,
}

pub fn services() -> Vec<Service> {
    let out = run("launchctl", &["list"]).unwrap_or_default();
    out.lines()
        .skip(1)
        .filter_map(|l| {
            let mut f = l.split('\t');
            let pid = f.next()?;
            let status = f.next()?;
            let label = f.next()?.trim().to_string();
            Some(Service {
                label,
                pid: pid.parse().ok(),
                status: status.parse().unwrap_or(0),
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct StartupItem {
    pub label: String,
    pub scope: &'static str,
    pub file: PathBuf,
    pub loaded: bool,
    pub editable: bool,
}

pub fn user_agents_dir() -> PathBuf {
    home().join("Library/LaunchAgents")
}

pub fn startup_items() -> Vec<StartupItem> {
    let loaded: HashSet<String> = services().into_iter().map(|s| s.label).collect();
    let dirs: [(PathBuf, &'static str); 3] = [
        (user_agents_dir(), "User agent"),
        (PathBuf::from("/Library/LaunchAgents"), "System agent"),
        (PathBuf::from("/Library/LaunchDaemons"), "System daemon"),
    ];
    let mut items = Vec::new();
    for (dir, scope) in dirs {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("plist") {
                continue;
            }
            let label = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            items.push(StartupItem {
                loaded: loaded.contains(&label),
                editable: scope == "User agent",
                label,
                scope,
                file: path,
            });
        }
    }
    items.sort_by(|a, b| {
        a.scope
            .cmp(b.scope)
            .then(a.label.to_lowercase().cmp(&b.label.to_lowercase()))
    });
    items
}

fn valid_label(label: &str) -> Result<(), String> {
    if !label.is_empty()
        && label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    {
        Ok(())
    } else {
        Err("Invalid service label".into())
    }
}

/// Load or unload one of the user's own launch agents. System agents are refused.
pub fn toggle_startup(file: &Path, enable: bool) -> Result<(), String> {
    let dir = user_agents_dir();
    let ok = file.parent() == Some(dir.as_path())
        && file.extension().and_then(|x| x.to_str()) == Some("plist")
        && !file.to_string_lossy().contains("..");
    if !ok {
        return Err("Only your own launch agents in ~/Library/LaunchAgents can be changed".into());
    }
    if !file.exists() {
        return Err("Launch agent not found".into());
    }
    let path = file.to_string_lossy();
    run(
        "launchctl",
        &[if enable { "load" } else { "unload" }, "-w", &path],
    )
    .map(|_| ())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    Restart,
    Stop,
}

pub fn control_service(label: &str, action: ServiceAction) -> Result<(), String> {
    valid_label(label)?;
    let uid = uid();
    let target = format!("gui/{uid}/{label}");
    let plist = user_agents_dir().join(format!("{label}.plist"));
    match action {
        ServiceAction::Restart if plist.exists() => {
            let _ = run("launchctl", &["bootout", &target]);
            run(
                "launchctl",
                &["bootstrap", &format!("gui/{uid}"), &plist.to_string_lossy()],
            )?;
            run("launchctl", &["kickstart", &target]).map(|_| ())
        }
        ServiceAction::Restart => run("launchctl", &["kickstart", "-k", &target]).map(|_| ()),
        ServiceAction::Stop => run("launchctl", &["kill", "TERM", &target]).map(|_| ()),
    }
}

// ---------- processes ----------

pub fn signal_process(pid: u32, force: bool) -> Result<(), String> {
    send_signal(pid, if force { libc::SIGKILL } else { libc::SIGTERM })
}

/// Send any signal (TERM, KILL, STOP to suspend, CONT to resume). launchd and the kernel are refused.
pub fn send_signal(pid: u32, sig: i32) -> Result<(), String> {
    if pid <= 1 {
        return Err("Refusing to signal launchd or the kernel".into());
    }
    if unsafe { libc::kill(pid as i32, sig) } == 0 {
        Ok(())
    } else {
        Err(format!("pid {pid}: {}", std::io::Error::last_os_error()))
    }
}

/// `ps` state letters for a process ("S", "R", "T" = stopped…), or None if it is gone.
pub fn process_state(pid: u32) -> Option<String> {
    let out = run("ps", &["-o", "stat=", "-p", &pid.to_string()]).ok()?;
    let s = out.trim().to_string();
    (!s.is_empty()).then_some(s)
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

pub fn connections() -> Vec<Connection> {
    let out = run("lsof", &["-nP", "-i", "-F", "pcPn"]).unwrap_or_default();
    let (mut pid, mut cmd, mut proto) = (0, String::new(), String::new());
    let mut rows = Vec::new();
    for line in out.lines() {
        let Some(tag) = line.chars().next() else {
            continue;
        };
        let val = &line[1..];
        match tag {
            'p' => pid = val.parse().unwrap_or(0),
            'c' => cmd = val.to_string(),
            'P' => proto = val.to_string(),
            'n' => {
                let (endpoint, state) = match val.rfind(" (") {
                    Some(i) if val.ends_with(')') => {
                        (val[..i].to_string(), val[i + 2..val.len() - 1].to_string())
                    }
                    _ => (val.to_string(), String::new()),
                };
                rows.push(Connection {
                    pid,
                    process: cmd.clone(),
                    proto: proto.clone(),
                    endpoint,
                    state,
                });
            }
            _ => {}
        }
    }
    rows
}

// ---------- installed apps ----------

#[derive(Clone, Debug)]
pub struct InstalledApp {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub bytes: Option<u64>,
}

fn plist_string(info: &Path, key: &str) -> Option<String> {
    run(
        "plutil",
        &["-extract", key, "raw", "-o", "-", &info.to_string_lossy()],
    )
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
}

pub fn installed_apps() -> Vec<InstalledApp> {
    let mut apps = Vec::new();
    for dir in [PathBuf::from("/Applications"), home().join("Applications")] {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("app") {
                continue;
            }
            let info = path.join("Contents/Info.plist");
            apps.push(InstalledApp {
                name: path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                version: plist_string(&info, "CFBundleShortVersionString")
                    .unwrap_or_else(|| "—".into()),
                path,
                bytes: None,
            });
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

pub fn app_sizes(paths: &[PathBuf]) -> Vec<Option<u64>> {
    paths
        .iter()
        .map(|p| {
            run("du", &["-sk", &p.to_string_lossy()])
                .ok()
                .and_then(|o| o.split_whitespace().next()?.parse::<u64>().ok())
                .map(|k| k * 1024)
        })
        .collect()
}

/// Move an app bundle to the Trash through Finder. Limited to /Applications and ~/Applications.
pub fn trash_app(path: &Path) -> Result<(), String> {
    let s = path.to_string_lossy();
    let roots = [PathBuf::from("/Applications"), home().join("Applications")];
    if path.extension().and_then(|x| x.to_str()) != Some("app")
        || !roots.iter().any(|r| path.parent() == Some(r.as_path()))
        || s.contains("..")
    {
        return Err(
            "Only .app bundles inside /Applications or ~/Applications can be removed".into(),
        );
    }
    if !path.exists() {
        return Err("App not found".into());
    }
    let script = format!(
        "tell application \"Finder\" to delete (POSIX file \"{}\" as alias)",
        s.replace('\\', "\\\\").replace('"', "\\\"")
    );
    run("osascript", &["-e", &script]).map(|_| ())
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

pub fn power() -> Power {
    let batt = run("pmset", &["-g", "batt"]).unwrap_or_default();
    let ioreg = run("ioreg", &["-rn", "AppleSmartBattery", "-w0"]).unwrap_or_default();
    let num = |k: &str| -> Option<i64> {
        let needle = format!("\"{k}\" = ");
        let i = ioreg.find(&needle)? + needle.len();
        let digits: String = ioreg[i..]
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '-')
            .collect();
        let v: i128 = digits.parse().ok()?;
        Some(if v > 9_000_000_000_000_000_000 {
            (v - (1i128 << 64)) as i64
        } else {
            v as i64
        })
    };
    let voltage = num("Voltage");
    let amperage = num("Amperage");
    let design = num("DesignCapacity");
    let max = num("NominalChargeCapacity").or_else(|| num("AppleRawMaxCapacity"));
    let percent = batt
        .split(['\t', ';'])
        .find_map(|p| p.trim().strip_suffix('%')?.parse().ok());
    let status = batt
        .split(';')
        .nth(1)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    let pm = run("pmset", &["-g"]).unwrap_or_default();
    let therm = run("pmset", &["-g", "therm"]).unwrap_or_default();
    Power {
        has_battery: batt.contains("InternalBattery"),
        source: batt.split('\'').nth(1).unwrap_or("unknown").to_string(),
        percent,
        status,
        watts: voltage.zip(amperage).map(|(v, a)| (v * a) as f64 / 1e6),
        voltage: voltage.map(|v| v as f64 / 1000.),
        cycles: num("CycleCount"),
        health: design
            .zip(max)
            .filter(|(d, _)| *d > 0)
            .map(|(d, m)| m as f64 / d as f64 * 100.),
        temperature: num("Temperature").map(|t| t as f64 / 100.),
        low_power_mode: pm
            .lines()
            .any(|l| l.trim_start().starts_with("lowpowermode") && l.trim_end().ends_with('1')),
        thermal: if therm.contains("No thermal warning level has been recorded") {
            "Nominal".into()
        } else {
            therm.lines().last().unwrap_or("unknown").trim().to_string()
        },
    }
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

pub fn reveal_in_finder(path: &str) {
    let _ = Command::new("open").args(["-R", path]).spawn();
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

/// Sequential 512 MB write and read with the unified buffer cache bypassed (F_NOCACHE).
fn disk_speed() -> (f64, f64) {
    const SIZE: usize = 512 * 1024 * 1024;
    const CHUNK: usize = 8 * 1024 * 1024;
    let path = std::env::temp_dir().join(format!("otm-bench-{}.bin", std::process::id()));
    let block = vec![0xA5u8; CHUNK];
    let write = (|| -> std::io::Result<f64> {
        let mut f = fs::File::create(&path)?;
        unsafe { libc::fcntl(f.as_raw_fd(), libc::F_NOCACHE, 1) };
        let start = Instant::now();
        for _ in 0..SIZE / CHUNK {
            f.write_all(&block)?;
        }
        f.sync_all()?;
        Ok(SIZE as f64 / start.elapsed().as_secs_f64() / 1e6)
    })();
    let read = (|| -> std::io::Result<f64> {
        let mut f = fs::File::open(&path)?;
        unsafe { libc::fcntl(f.as_raw_fd(), libc::F_NOCACHE, 1) };
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
