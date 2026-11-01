//! Linux checks for `otm --selftest`: the operating system / your processes split and owners, autostart
//! entries, your systemd user units, signals on throwaway processes, sockets, folder sizes and cores.

use super::{host, startup::config_home};
use std::{net::TcpListener, path::PathBuf, thread::sleep, time::Duration};

use crate::{
    categories,
    metrics::{ProcessInfo, Snapshot},
    selftest::Report,
    sysops::{self, ServiceAction},
};

const AUTOSTART: &str = "otm-selftest.desktop";
const UNIT: &str = "otm-selftest.service";

/// Init (systemd) and kthreadd, the parent of kernel threads, hold all but a few rows of the tree.
pub fn init_holds_tree(descendants: usize, kernel: usize, rows: usize) -> bool {
    descendants + kernel + 3 >= rows
}

/// Kernel threads have neither a resident-memory reading nor an executable on Linux.
pub fn unmeasured(p: &ProcessInfo) -> bool {
    p.memory == 0 && !p.path.is_empty()
}

fn alive(pid: u32) -> bool {
    host::run(&["kill", "-0", &pid.to_string()]).is_ok()
}

/// A disposable `sleep` for the quit checks, started on the host so its pid is the one the process table
/// and the signals use (inside a Flatpak a child of this process has a sandbox pid).
pub struct Throwaway(u32);

impl Throwaway {
    pub fn start() -> Option<Self> {
        host::run(&["sh", "-c", "sleep 600 >/dev/null 2>&1 & echo $!"])
            .ok()
            .and_then(|out| out.trim().parse().ok())
            .map(Self)
    }

    pub fn pid(&self) -> u32 {
        self.0
    }

    /// Whether the process is gone within two seconds of the signal.
    pub fn ended(self) -> bool {
        (0..20).any(|_| {
            sleep(Duration::from_millis(100));
            !alive(self.0)
        })
    }
}

pub fn app_checks(r: &mut Report, snap: &Snapshot) {
    let started = std::time::Instant::now();
    let kernel = host::run(&["uname", "-s"]);
    let route = if host::sandboxed() {
        "flatpak-spawn --host"
    } else {
        "direct"
    };
    r.check(
        "host commands",
        kernel.is_ok_and(|os| os.trim() == "Linux"),
        format!(
            "{route} round trip took {} ms",
            started.elapsed().as_millis()
        ),
    );
    r.check(
        "host files",
        host::read("/etc/os-release").is_ok_and(|text| text.contains("ID=")),
        format!("reading /etc/os-release through the {route} route"),
    );

    // Every row is a process, not one of its threads: a thread's Tgid is its process's pid.
    let threads: Vec<u32> = snap
        .processes
        .iter()
        .filter(|p| {
            host::read(format!("/proc/{}/status", p.pid)).is_ok_and(|status| {
                status
                    .lines()
                    .find_map(|l| l.strip_prefix("Tgid:"))
                    .and_then(|v| v.trim().parse::<u32>().ok())
                    .is_some_and(|tgid| tgid != p.pid)
            })
        })
        .map(|p| p.pid)
        .collect();
    r.check(
        "no threads as processes",
        threads.is_empty(),
        format!(
            "{} of {} rows are threads: {:?}",
            threads.len(),
            snap.processes.len(),
            &threads[..threads.len().min(5)]
        ),
    );

    let owners = categories::owners(&snap.processes);
    let me = super::own_pid();
    let mine = owners.get(&me).cloned().unwrap_or_default();
    let system = snap
        .processes
        .iter()
        .filter(|p| owners.get(&p.pid).map(String::as_str) == Some(categories::OS_OWNER))
        .count();
    // Started by a system service (a CI runner), this process rightly belongs to the OS; only a
    // user session (a user@ or session- cgroup) has an app to own it.
    let in_session = host::read("/proc/self/cgroup")
        .map(|c| c.contains("/user@") || c.contains("/session-") || !c.contains(".service"))
        .unwrap_or(true);
    r.check(
        "owners",
        !mine.is_empty() && (mine != categories::OS_OWNER || !in_session) && system > 0,
        format!(
            "this process ({me}) is owned by {mine:?} (user session: {in_session}); {system} operating-system processes"
        ),
    );
    autostart(r);
    user_unit(r);
    signals(r, snap);
    sockets(r);
    folder_size(r);
    cores(r);
}

fn autostart(r: &mut Report) {
    let dir = config_home().join("autostart");
    let file = dir.join(AUTOSTART);
    let written = host::create_dir_all(&dir).and_then(|_| {
        host::write(
            &file,
            b"[Desktop Entry]\nType=Application\nName=OTM Selftest\nExec=/bin/sleep 3600\n",
        )
    });
    if let Err(e) = written {
        r.check(
            "autostart toggle",
            false,
            format!("cannot write {}: {e}", file.display()),
        );
        return;
    }
    let listed = || sysops::startup_items().into_iter().find(|s| s.file == file);
    let start = listed().map(|s| s.loaded);
    let off = sysops::toggle_startup(&file, false);
    let disabled = listed().is_some_and(|s| !s.loaded);
    let on = sysops::toggle_startup(&file, true);
    let enabled = listed().is_some_and(|s| s.loaded);
    host::remove_file(&file);
    r.check(
        "autostart toggle",
        start == Some(true) && off.is_ok() && disabled && on.is_ok() && enabled,
        format!("loaded at start {start:?}; after disable {disabled}; after enable {enabled}"),
    );
}

fn user_unit(r: &mut Report) {
    const NAME: &str = "user unit restart/stop x3";
    if let Err(reason) = host::run(&["systemctl", "--user", "show-environment"]) {
        println!("SKIP {NAME}: no systemd user manager here ({reason})");
        return;
    }
    let dir = config_home().join("systemd/user");
    let file: PathBuf = dir.join(UNIT);
    let written = host::create_dir_all(&dir)
        .and_then(|_| host::write(&file, b"[Service]\nExecStart=/bin/sleep 3600\n"));
    if let Err(e) = written {
        r.check(NAME, false, format!("cannot write {}: {e}", file.display()));
        return;
    }
    host::run(&["systemctl", "--user", "daemon-reload"]).ok();
    let pid_of = || {
        sysops::services()
            .into_iter()
            .find(|s| s.label == UNIT)
            .and_then(|s| s.pid)
    };
    let mut cycles = Vec::new();
    let mut errors = Vec::new();
    for _ in 0..3 {
        let res = sysops::control_service(UNIT, ServiceAction::Restart);
        sleep(Duration::from_millis(1000));
        let pid = pid_of();
        let up = pid.is_some_and(alive);
        let stop = sysops::control_service(UNIT, ServiceAction::Stop);
        sleep(Duration::from_millis(1000));
        let gone = pid.is_some_and(|p| !alive(p));
        if let Err(e) = res.as_ref().and(stop.as_ref()) {
            errors.push(e.clone());
        }
        cycles.push((res.is_ok() && up, stop.is_ok() && gone, pid));
    }
    host::remove_file(&file);
    host::run(&["systemctl", "--user", "daemon-reload"]).ok();
    r.check(
        NAME,
        cycles.iter().all(|(a, b, _)| *a && *b),
        format!("(started, stopped, pid): {cycles:?}; errors: {errors:?}"),
    );
}

fn signals(r: &mut Report, snap: &Snapshot) {
    let started = host::run(&["sh", "-c", "sleep 600 >/dev/null 2>&1 & echo $!"]);
    let Some(pid) = started.ok().and_then(|out| out.trim().parse::<u32>().ok()) else {
        r.check(
            "suspend and resume",
            false,
            "cannot start a test process on the host",
        );
        return;
    };
    let stop = sysops::send_signal(pid, libc::SIGSTOP);
    sleep(Duration::from_millis(150));
    let suspended = sysops::process_state(pid).as_deref() == Some("T");
    let cont = sysops::send_signal(pid, libc::SIGCONT);
    sleep(Duration::from_millis(150));
    let resumed = sysops::process_state(pid).is_some_and(|s| s != "T");
    let kill = sysops::signal_process(pid, true);
    r.check(
        "suspend and resume",
        stop.is_ok() && suspended && cont.is_ok() && resumed && kill.is_ok(),
        format!("pid {pid}: suspended {suspended}, resumed {resumed}, killed {kill:?}"),
    );
    match snap
        .processes
        .iter()
        .find(|p| p.path.is_empty() && p.ppid == Some(2))
    {
        Some(kthread) => {
            let refused = sysops::signal_process(kthread.pid, false);
            r.check(
                "refuse kernel thread",
                refused.is_err(),
                format!(
                    "kernel thread {} ({}): {refused:?}",
                    kthread.name, kthread.pid
                ),
            );
        }
        None => println!("SKIP refuse kernel thread: no kernel thread is visible to this process"),
    }
}

fn sockets(r: &mut Report) {
    if host::sandboxed() {
        // The listing reads the host's sockets; a Flatpak without network access has its own network
        // namespace, so a socket it opens is never one of them.
        println!(
            "SKIP connections: this process's sockets live in the sandbox's own network namespace"
        );
        return;
    }
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        r.check("connections", false, "cannot open a test socket");
        return;
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let me = super::own_pid();
    let conns = sysops::connections();
    let mine = conns
        .iter()
        .find(|c| c.pid == me && c.endpoint.ends_with(&format!(":{port}")));
    r.check(
        "connections",
        mine.is_some(),
        format!(
            "socket on port {port} of this process listed as {:?}; {} sockets in all",
            mine.map(|c| (&c.proto, &c.state)),
            conns.len()
        ),
    );
}

fn folder_size(r: &mut Report) {
    let dir = sysops::data_dir().join(format!("otm-selftest-size-{}", std::process::id()));
    let blob = 2 * 1024 * 1024;
    let written = host::create_dir_all(&dir)
        .and_then(|_| host::write(dir.join("blob.bin"), &vec![1u8; blob]));
    let sizes = if written.is_ok() {
        // No size known up front, so this measures the folder itself.
        sysops::app_sizes(&[sysops::InstalledApp {
            name: "selftest".into(),
            version: String::new(),
            path: dir.clone(),
            bytes: None,
            source: "Manual",
        }])
    } else {
        vec![None]
    };
    host::remove_dir_all(&dir);
    r.check(
        "folder size",
        sizes[0].is_some_and(|b| b >= blob as u64),
        format!("a 2 MB folder in the data folder measures {:?}", sizes[0]),
    );
}

fn cores(r: &mut Report) {
    let info = sysops::system_info();
    let logical = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);
    r.check(
        "cpu cores",
        info.physical_cores >= 1
            && info.physical_cores <= logical
            && info.performance_cores + info.efficiency_cores <= info.physical_cores,
        format!(
            "{} physical cores ({}P/{}E) of {logical} logical CPUs",
            info.physical_cores, info.performance_cores, info.efficiency_cores
        ),
    );
}
