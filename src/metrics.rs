use std::collections::{HashMap, VecDeque};

use sysinfo::{
    CpuRefreshKind, Disks, MemoryRefreshKind, Networks, ProcessRefreshKind, ProcessesToUpdate,
    RefreshKind, System, UpdateKind, Users,
};

pub const HISTORY: usize = 90;

#[derive(Clone, Debug)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: Option<u32>,
    pub name: String,
    pub user: String,
    pub cpu: f32,
    pub memory: u64,
    pub disk_read: u64,
    pub disk_write: u64,
    pub state: String,
    pub run_time: u64,
    pub path: String,
    pub is_self: bool,
    /// The app this process belongs to, or "Operating System" for macOS's own processes.
    pub owner_app: String,
}

#[derive(Clone, Debug, Default)]
pub struct DiskInfo {
    pub mount: String,
    pub total: u64,
    pub available: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub cpu: f32,
    pub cores: Vec<f32>,
    pub memory_used: u64,
    pub memory_total: u64,
    pub swap_used: u64,
    pub net_down: u64,
    pub net_up: u64,
    pub disks: Vec<DiskInfo>,
    pub processes: Vec<ProcessInfo>,
    pub uptime: u64,
    pub host: String,
    pub os: String,
    pub cpu_brand: String,
}

#[derive(Clone, Debug, Default)]
pub struct History {
    pub cpu: VecDeque<f32>,
    pub memory: VecDeque<f32>,
    pub net_down: VecDeque<f32>,
    pub net_up: VecDeque<f32>,
}

impl History {
    pub fn push(&mut self, s: &Snapshot) {
        let mem = if s.memory_total == 0 {
            0.0
        } else {
            s.memory_used as f32 / s.memory_total as f32 * 100.0
        };
        for (series, value) in [
            (&mut self.cpu, s.cpu),
            (&mut self.memory, mem),
            (&mut self.net_down, s.net_down as f32),
            (&mut self.net_up, s.net_up as f32),
        ] {
            series.push_back(value);
            while series.len() > HISTORY {
                series.pop_front();
            }
        }
    }
}

pub struct Sampler {
    system: System,
    networks: Networks,
    disks: Disks,
    users: Users,
    user_names: HashMap<String, String>,
    ps: HashMap<u32, PsRow>,
    self_pid: u32,
}

struct PsRow {
    ppid: u32,
    user: String,
    cpu: f32,
    memory: u64,
    path: String,
}

impl Sampler {
    pub fn new() -> Self {
        let system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        Self {
            system,
            networks: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            users: Users::new_with_refreshed_list(),
            user_names: HashMap::new(),
            ps: HashMap::new(),
            self_pid: std::process::id(),
        }
    }

    /// Owner, CPU and resident memory for every process from `ps`, which (unlike sysinfo's
    /// task APIs) can read processes owned by other users without root.
    fn refresh_ps(&mut self) {
        let out = std::process::Command::new("ps")
            .args(["-axo", "pid=,ppid=,user=,pcpu=,rss=,comm="])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        self.ps = out
            .lines()
            .filter_map(|l| {
                // Five space-separated fields, then the executable path, which may contain spaces.
                let mut rest = l.trim_start();
                let mut field = || {
                    let (f, r) = rest.split_once(char::is_whitespace)?;
                    rest = r.trim_start();
                    Some(f)
                };
                let pid = field()?.parse().ok()?;
                let ppid = field()?.parse().ok()?;
                let user = field()?.to_string();
                let cpu = field()?.parse().ok()?;
                let rss: u64 = field()?.parse().ok()?;
                // Login shells are listed as "-/bin/zsh".
                let path = rest.trim_end().trim_start_matches('-').to_string();
                Some((
                    pid,
                    PsRow {
                        ppid,
                        user,
                        cpu,
                        memory: rss * 1024,
                        path,
                    },
                ))
            })
            .collect();
    }

    fn ps_owner(&self, pid: u32) -> String {
        self.ps
            .get(&pid)
            .map(|r| r.user.clone())
            .unwrap_or_else(|| "unknown".into())
    }

    fn user_name(&mut self, pid: u32, uid: Option<&sysinfo::Uid>) -> String {
        let Some(uid) = uid else {
            return self.ps_owner(pid);
        };
        let key = uid.to_string();
        if let Some(name) = self.user_names.get(&key) {
            return name.clone();
        }
        let name = self
            .users
            .get_user_by_id(uid)
            .map(|u| u.name().to_string())
            .unwrap_or_else(|| format!("uid {key}"));
        self.user_names.insert(key, name.clone());
        name
    }

    pub fn sample(&mut self) -> Snapshot {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_disk_usage()
                .with_user(UpdateKind::OnlyIfNotSet)
                .with_exe(UpdateKind::OnlyIfNotSet),
        );
        self.networks.refresh(true);
        self.disks.refresh(true);

        let (net_down, net_up) = self
            .networks
            .iter()
            .filter(|(name, _)| !name.starts_with("lo"))
            .fold((0, 0), |(d, u), (_, n)| {
                (d + n.received(), u + n.transmitted())
            });

        let raw: Vec<_> = self
            .system
            .processes()
            .values()
            .map(|p| {
                (
                    p.pid().as_u32(),
                    p.parent().map(|x| x.as_u32()),
                    p.name().to_string_lossy().into_owned(),
                    p.user_id().cloned(),
                    p.cpu_usage(),
                    p.memory(),
                    p.disk_usage(),
                    p.status().to_string(),
                    p.run_time(),
                    p.exe().map(|e| e.display().to_string()).unwrap_or_default(),
                )
            })
            .collect();

        self.refresh_ps();
        let processes = raw
            .into_iter()
            .map(
                |(pid, ppid, name, uid, cpu, memory, disk, state, run_time, path)| {
                    // sysinfo reports 0 memory for processes it may not inspect; ps can still read them.
                    let (cpu, memory) = match self.ps.get(&pid) {
                        Some(ps) if memory == 0 => (ps.cpu, ps.memory),
                        _ => (cpu, memory),
                    };
                    // sysinfo cannot read the parent of other users' processes; ps can.
                    let ppid = ppid
                        .or_else(|| self.ps.get(&pid).map(|ps| ps.ppid).filter(|&pp| pp != pid));
                    // Same for the executable path of other users' processes.
                    let path = if path.is_empty() {
                        self.ps
                            .get(&pid)
                            .map(|ps| ps.path.clone())
                            .filter(|p| p.starts_with('/'))
                            .unwrap_or_default()
                    } else {
                        path
                    };
                    (
                        pid, ppid, name, uid, cpu, memory, disk, state, run_time, path,
                    )
                },
            )
            .collect::<Vec<_>>()
            .into_iter()
            .map(
                |(pid, ppid, name, uid, cpu, memory, disk, state, run_time, path)| ProcessInfo {
                    pid,
                    ppid,
                    name,
                    user: self.user_name(pid, uid.as_ref()),
                    cpu,
                    memory,
                    disk_read: disk.read_bytes,
                    disk_write: disk.written_bytes,
                    state,
                    run_time,
                    path,
                    is_self: pid == self.self_pid,
                    owner_app: String::new(),
                },
            )
            .collect::<Vec<_>>();
        let owners = crate::categories::owners(&processes);
        let processes: Vec<ProcessInfo> = processes
            .into_iter()
            .map(|mut p| {
                p.owner_app = owners.get(&p.pid).cloned().unwrap_or_default();
                p
            })
            .collect();

        let disks = self
            .disks
            .iter()
            .filter(|d| {
                let m = d.mount_point();
                m == std::path::Path::new("/") || m.starts_with("/Volumes/")
            })
            .map(|d| DiskInfo {
                mount: d.mount_point().display().to_string(),
                total: d.total_space(),
                available: d.available_space(),
            })
            .collect();

        Snapshot {
            cpu: self.system.global_cpu_usage(),
            cores: self.system.cpus().iter().map(|c| c.cpu_usage()).collect(),
            memory_used: self.system.used_memory(),
            memory_total: self.system.total_memory(),
            swap_used: self.system.used_swap(),
            net_down,
            net_up,
            disks,
            processes,
            uptime: System::uptime(),
            host: System::host_name().unwrap_or_default(),
            os: System::long_os_version().unwrap_or_default(),
            cpu_brand: self
                .system
                .cpus()
                .first()
                .map(|c| c.brand().to_string())
                .unwrap_or_default(),
        }
    }
}

pub fn fmt_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn fmt_duration(secs: u64) -> String {
    let (d, h, m, s) = (secs / 86_400, secs / 3600 % 24, secs / 60 % 60, secs % 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}:{s:02}")
    }
}
