//! Linux checks for `otm --selftest`: the operating system / your processes split and owners, autostart
//! entries, your systemd user units, signals on throwaway processes, sockets, folder sizes and cores.

use std::{fs, net::TcpListener, path::PathBuf, process::Command, thread::sleep, time::Duration};

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

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| sysops::home().join(".config"))
}

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

pub fn app_checks(r: &mut Report, snap: &Snapshot) {
    let owners = categories::owners(&snap.processes);
    let me = std::process::id();
    let mine = owners.get(&me).cloned().unwrap_or_default();
    let system = snap
        .processes
        .iter()
        .filter(|p| owners.get(&p.pid).map(String::as_str) == Some(categories::OS_OWNER))
        .count();
    r.check(
        "owners",
        !mine.is_empty() && mine != categories::OS_OWNER && system > 0,
        format!("this process ({me}) is owned by {mine:?}; {system} operating-system processes"),
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
    let written = fs::create_dir_all(&dir).and_then(|_| {
        fs::write(
            &file,
            "[Desktop Entry]\nType=Application\nName=OTM Selftest\nExec=/bin/sleep 3600\n",
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
    fs::remove_file(&file).ok();
    r.check(
        "autostart toggle",
        start == Some(true) && off.is_ok() && disabled && on.is_ok() && enabled,
        format!("loaded at start {start:?}; after disable {disabled}; after enable {enabled}"),
    );
}

fn user_unit(r: &mut Report) {
    const NAME: &str = "user unit restart/stop x3";
    if let Err(reason) = sysops::run("systemctl", &["--user", "show-environment"]) {
        println!("SKIP {NAME}: no systemd user manager here ({reason})");
        return;
    }
    let dir = config_home().join("systemd/user");
    let file = dir.join(UNIT);
    let written = fs::create_dir_all(&dir)
        .and_then(|_| fs::write(&file, "[Service]\nExecStart=/bin/sleep 3600\n"));
    if let Err(e) = written {
        r.check(NAME, false, format!("cannot write {}: {e}", file.display()));
        return;
    }
    sysops::run("systemctl", &["--user", "daemon-reload"]).ok();
    let pid_of = || {
        sysops::services()
            .into_iter()
            .find(|s| s.label == UNIT)
            .and_then(|s| s.pid)
    };
    let mut cycles = Vec::new();
    for _ in 0..3 {
        let res = sysops::control_service(UNIT, ServiceAction::Restart);
        sleep(Duration::from_millis(1000));
        let pid = pid_of();
        let up = pid.is_some_and(alive);
        let stop = sysops::control_service(UNIT, ServiceAction::Stop);
        sleep(Duration::from_millis(1000));
        let gone = pid.is_some_and(|p| !alive(p));
        cycles.push((res.is_ok() && up, stop.is_ok() && gone, pid));
    }
    fs::remove_file(&file).ok();
    sysops::run("systemctl", &["--user", "daemon-reload"]).ok();
    r.check(
        NAME,
        cycles.iter().all(|(a, b, _)| *a && *b),
        format!("(started, stopped, pid): {cycles:?}"),
    );
}

fn signals(r: &mut Report, snap: &Snapshot) {
    let mut child = Command::new("/bin/sleep").arg("600").spawn().unwrap();
    let pid = child.id();
    let stop = sysops::send_signal(pid, libc::SIGSTOP);
    sleep(Duration::from_millis(150));
    let suspended = sysops::process_state(pid).as_deref() == Some("T");
    let cont = sysops::send_signal(pid, libc::SIGCONT);
    sleep(Duration::from_millis(150));
    let resumed = sysops::process_state(pid).is_some_and(|s| s != "T");
    let kill = sysops::signal_process(pid, true);
    let status = child.wait().ok();
    r.check(
        "suspend and resume",
        stop.is_ok() && suspended && cont.is_ok() && resumed && kill.is_ok(),
        format!("pid {pid}: suspended {suspended}, resumed {resumed}, exit {status:?}"),
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
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        r.check("connections", false, "cannot open a test socket");
        return;
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let me = std::process::id();
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
    let dir = std::env::temp_dir().join(format!("otm-selftest-size-{}", std::process::id()));
    let blob = 2 * 1024 * 1024;
    let written =
        fs::create_dir_all(&dir).and_then(|_| fs::write(dir.join("blob.bin"), vec![1u8; blob]));
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
    fs::remove_dir_all(&dir).ok();
    r.check(
        "folder size",
        sizes[0].is_some_and(|b| b >= blob as u64),
        format!("a 2 MB temp folder measures {:?}", sizes[0]),
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
