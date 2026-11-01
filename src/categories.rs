//! "Category" grouping: the operating system's own processes versus apps (one section per app) and
//! command-line tools. The group names and each process's group come from `crate::platform`.

use std::collections::HashMap;

use crate::{metrics::ProcessInfo, platform::CAPS};

/// The group of the operating system's own processes.
pub const SYSTEM_GROUP: &str = CAPS.system_group;
pub const APPS: &str = CAPS.apps_group;

/// Top-level groups in display order.
pub const TOPS: &[&str] = CAPS.groups;

pub const OTHER_SYSTEM: &str = "Other System Services";

/// The owner shown for processes that belong to the operating system rather than to any app.
pub const OS_OWNER: &str = "Operating System";

/// The app (or tool) each process belongs to: the app bundle for app processes and their helpers,
/// the outermost command-line tool for tool processes, and "Operating System" for the system's own.
pub fn owners(processes: &[ProcessInfo]) -> HashMap<u32, String> {
    let cat = categorize(processes);
    let by_pid: HashMap<u32, &ProcessInfo> = processes.iter().map(|p| (p.pid, p)).collect();
    processes
        .iter()
        .map(|p| {
            let owner = match cat.get(&p.pid) {
                Some((top, _)) if *top == SYSTEM_GROUP => OS_OWNER.to_string(),
                Some((top, app)) if *top == APPS => app.clone(),
                _ => {
                    // A tool: name it after the outermost non-system process it runs under.
                    let mut top = p;
                    let mut hops = 0;
                    while let Some(parent) = top
                        .ppid
                        .filter(|&pp| pp > 1 && pp != top.pid && hops < 64)
                        .and_then(|pp| by_pid.get(&pp).copied())
                        .filter(|pa| cat.get(&pa.pid).is_some_and(|c| c.0 != SYSTEM_GROUP))
                    {
                        top = parent;
                        hops += 1;
                    }
                    top.name.clone()
                }
            };
            (p.pid, owner)
        })
        .collect()
}

/// (top group, section) for every process. A system process started under one of your apps or
/// tools (a shell in a terminal, a helper an app spawned) is filed with it, because that is what it
/// belongs to.
pub fn categorize(processes: &[ProcessInfo]) -> HashMap<u32, (&'static str, String)> {
    let by_pid: HashMap<u32, &ProcessInfo> = processes.iter().map(|p| (p.pid, p)).collect();
    let own: HashMap<u32, (&'static str, String)> = processes
        .iter()
        .map(|p| (p.pid, crate::platform::classify(p)))
        .collect();
    processes
        .iter()
        .map(|p| {
            let mine = own[&p.pid].clone();
            if mine.0 != SYSTEM_GROUP {
                return (p.pid, mine);
            }
            // A system binary started by an app or a tool (a shell in a terminal, `grep` in a script)
            // belongs with that app or tool: the nearest non-system ancestor decides. Stop at init.
            let mut cur = p.ppid;
            let mut hops = 0;
            while let Some(pp) = cur.filter(|&pp| pp > 1 && pp != p.pid && hops < 64) {
                if let Some(c) = own.get(&pp).filter(|c| c.0 != SYSTEM_GROUP) {
                    return (p.pid, c.clone());
                }
                cur = by_pid
                    .get(&pp)
                    .and_then(|a| a.ppid)
                    .filter(|&next| next != pp);
                hops += 1;
            }
            (p.pid, mine)
        })
        .collect()
}
