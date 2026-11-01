//! Linux checks for `otm --selftest`: the operating system / your processes split and owners.

use crate::{categories, metrics::Snapshot, selftest::Report};

pub fn app_checks(r: &mut Report, snap: &Snapshot) {
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
