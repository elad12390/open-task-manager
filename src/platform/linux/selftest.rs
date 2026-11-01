//! Linux checks for `otm --selftest`: the operating system / your processes split and owners.

use super::host;
use crate::{categories, metrics::Snapshot, selftest::Report};

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

    let owners = categories::owners(&snap.processes);
    let me = std::process::id();
    let mine = owners.get(&me).cloned().unwrap_or_default();
    let my_name = snap
        .processes
        .iter()
        .find(|p| p.pid == me)
        .map(|p| p.name.clone())
        .unwrap_or_default();
    let system = snap
        .processes
        .iter()
        .filter(|p| owners.get(&p.pid).map(String::as_str) == Some(categories::OS_OWNER))
        .count();
    r.check(
        "owners",
        mine == my_name && system > 0,
        format!("this process ({me}) is owned by {mine:?}; {system} operating-system processes"),
    );
}
