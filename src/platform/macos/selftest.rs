//! macOS checks for `otm --selftest`: launchd's split of the process tree, macOS's own categories,
//! owner apps, launch agents and services, the Trash, sockets and power.

use std::{collections::HashSet, fs, path::PathBuf, thread::sleep, time::Duration};

use crate::{
    columns::{GroupBy, TableLayout},
    metrics::Snapshot,
    processes::{Row, build_rows},
    selftest::Report,
    sysops::{self, ServiceAction},
};

const LABEL: &str = "test.otm.native.disposable";

fn service_pid() -> Option<u32> {
    sysops::services()
        .into_iter()
        .find(|s| s.label == LABEL)
        .and_then(|s| s.pid)
}

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

pub fn app_checks(r: &mut Report, snap: &Snapshot) {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let mut layout = TableLayout {
        group_by: GroupBy::Parent,
        ..Default::default()
    };
    let tree = build_rows(&snap.processes, &layout, "", &HashSet::new());
    let nodes: Vec<(u32, Option<u32>, usize, usize)> = tree
        .iter()
        .filter_map(|row| match row {
            Row::Process {
                info,
                depth,
                descendants,
                ..
            } => Some((info.pid, info.ppid, *depth, *descendants)),
            _ => None,
        })
        .collect();
    // Under launchd: an "Operating System" section, then "Your Processes".
    let mut section: Option<String> = None;
    let mut heads: Vec<(String, usize, u64)> = Vec::new();
    let mut members: std::collections::HashMap<String, (usize, usize, u64)> =
        std::collections::HashMap::new();
    // Inside each section, a node per macOS subsystem / per owning app (depth 2), processes at depth 3.
    let cats = crate::categories::categorize(&snap.processes);
    let mut sub: Option<String> = None;
    let mut sub_wrong: Vec<String> = Vec::new();
    let mut subs_seen: Vec<(String, String)> = Vec::new();
    let mut placed: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    for row in &tree {
        match row {
            Row::Group {
                depth: 1,
                label,
                count,
                memory,
                ..
            } => {
                section = Some(label.clone());
                heads.push((label.clone(), *count, *memory));
            }
            Row::Group {
                depth: 2, label, ..
            } => {
                sub = Some(label.clone());
                subs_seen.push((section.clone().unwrap_or_default(), label.clone()));
            }
            Row::Process {
                info,
                depth,
                tree_memory,
                ..
            } => {
                if *depth <= 1 {
                    section = None;
                }
                if let (Some(sec), Some(g), 3) = (&section, &sub, *depth) {
                    placed.insert(info.pid, g.clone());
                    let expected = if sec == "Operating System" {
                        cats.get(&info.pid).map(|c| c.1.clone()).unwrap_or_default()
                    } else {
                        info.owner_app.clone()
                    };
                    if &expected != g {
                        sub_wrong.push(format!("{} under {g} (expected {expected})", info.name));
                    }
                }
                if let (Some(sec), 3) = (&section, *depth) {
                    let e = members.entry(sec.clone()).or_default();
                    e.0 += 1;
                    e.2 += tree_memory;
                    if (sec == "Operating System")
                        != (info.owner_app == crate::categories::OS_OWNER)
                    {
                        e.1 += 1;
                    }
                }
            }
            _ => {}
        }
    }
    let launchd_pos = tree
        .iter()
        .position(|r| matches!(r, Row::Process { info, .. } if info.pid == 1));
    let next_two: Vec<String> = launchd_pos
        .map(|i| {
            tree[i + 1..]
                .iter()
                .filter_map(|r| match r {
                    Row::Group {
                        label, depth: 1, ..
                    } => Some(label.clone()),
                    _ => None,
                })
                .take(2)
                .collect()
        })
        .unwrap_or_default();
    let sums_match = heads
        .iter()
        .all(|(label, _, memory)| members.get(label).is_some_and(|m| m.2 == *memory));
    let wrong: usize = members.values().map(|m| m.1).sum();
    r.check(
        "launchd split",
        next_two == ["Operating System", "Your Processes"]
            && matches!(tree.get(launchd_pos.unwrap_or(0) + 1), Some(Row::Group { .. }))
            && wrong == 0
            && sums_match
            && heads.iter().map(|h| h.1).sum::<usize>() + 1 == nodes.len(),
        format!(
            "under launchd: {:?}; {wrong} processes in the wrong section; section memory = sum of its branches: {sums_match}",
            heads.iter().map(|h| format!("{} ({} processes, {})", h.0, h.1, crate::metrics::fmt_bytes(h.2))).collect::<Vec<_>>()
        ),
    );
    let named = |name: &str| snap.processes.iter().find(|p| p.name == name);
    let spot = |name: &str| {
        named(name).map(|p| {
            format!(
                "{name}: owner {} -> under {:?}",
                p.owner_app,
                placed.get(&p.pid)
            )
        })
    };
    let steam_ok = named("ipcserver").is_none_or(|p| {
        p.owner_app == "Steam" && placed.get(&p.pid).map(String::as_str) == Some("Steam")
    });
    let books_ok = named("BooksThumbnail").is_none_or(|p| {
        p.owner_app == "Books" && placed.get(&p.pid).map(String::as_str) == Some("Books")
    });
    let yours: Vec<&String> = subs_seen
        .iter()
        .filter(|s| s.0 == "Your Processes")
        .map(|s| &s.1)
        .collect();
    let os_subs: Vec<&String> = subs_seen
        .iter()
        .filter(|s| s.0 == "Operating System")
        .map(|s| &s.1)
        .collect();
    r.check(
        "tree groups by owner",
        sub_wrong.is_empty() && steam_ok && books_ok && yours.len() > 5 && os_subs.len() > 5,
        format!(
            "{} owner nodes under Your Processes (e.g. {:?}), {} subsystem nodes under Operating System (e.g. {:?}); {} processes under the wrong node; {:?} {:?}",
            yours.len(),
            yours.iter().take(6).collect::<Vec<_>>(),
            os_subs.len(),
            os_subs.iter().take(4).collect::<Vec<_>>(),
            sub_wrong.len(),
            spot("ipcserver"),
            spot("BooksThumbnail"),
        ),
    );
    for w in sub_wrong.iter().take(5) {
        println!("     wrong node: {w}");
    }
    let deepest = nodes.iter().max_by_key(|n| n.2).map(|n| n.0).unwrap_or(0);
    let chain: Vec<String> = {
        let by_pid: std::collections::HashMap<u32, &crate::metrics::ProcessInfo> =
            snap.processes.iter().map(|p| (p.pid, p)).collect();
        let mut out = Vec::new();
        let mut cur = by_pid.get(&deepest).copied();
        while let Some(p) = cur {
            out.push(format!("{} ({})", p.name, p.pid));
            cur = p
                .ppid
                .filter(|&pp| pp != p.pid)
                .and_then(|pp| by_pid.get(&pp).copied());
        }
        out.reverse();
        out
    };
    println!("     deepest chain: {}", chain.join(" → "));
    // Category grouping: macOS (sections) / Apps (per app) / Command-line & Other.
    layout.group_by = GroupBy::Category;
    let cat_rows = build_rows(&snap.processes, &layout, "", &HashSet::new());
    let cats = crate::categories::categorize(&snap.processes);
    let mut tops: Vec<(String, usize, u64)> = Vec::new();
    let mut sections: Vec<(String, usize, u64)> = Vec::new();
    let mut shown_procs = 0;
    let mut deepest_proc = 0;
    let mut current_top = String::new();
    for row in &cat_rows {
        match row {
            Row::Group {
                depth: 0,
                label,
                count,
                memory,
                ..
            } => {
                current_top = label.clone();
                tops.push((label.clone(), *count, *memory));
            }
            Row::Group {
                depth: 1,
                label,
                count,
                memory,
                ..
            } if current_top == crate::categories::SYSTEM_GROUP => {
                sections.push((label.clone(), *count, *memory));
            }
            Row::Process { depth, .. } => {
                shown_procs += 1;
                deepest_proc = deepest_proc.max(*depth);
            }
            _ => {}
        }
    }
    let of = |pid: u32| cats.get(&pid).cloned().unwrap_or(("?", String::new()));
    let window_server = snap
        .processes
        .iter()
        .find(|p| p.name == "WindowServer")
        .map(|p| of(p.pid));
    let me = of(std::process::id());
    let misfiled_apps: Vec<String> = snap
        .processes
        .iter()
        .filter(|p| {
            p.path.contains(".app/")
                && !p.path.starts_with("/System/")
                && !p.path.starts_with("/Library/Apple/")
                && !p.path.contains("/Cryptexes/")
        })
        .filter_map(|p| {
            let app = p
                .path
                .split('/')
                .find_map(|s| s.strip_suffix(".app"))?
                .to_string();
            let c = of(p.pid);
            (c != (crate::categories::APPS, app.clone()))
                .then(|| format!("{} -> {:?} (expected {app})", p.name, c))
        })
        .collect();
    let native_under_launchd_not_macos = snap
        .processes
        .iter()
        .filter(|p| {
            p.ppid == Some(1)
                && (p.path.starts_with("/System/Library/") || p.path.starts_with("/usr/libexec/"))
        })
        .filter(|p| of(p.pid).0 != crate::categories::SYSTEM_GROUP)
        .count();
    let macos_mem: u64 = snap
        .processes
        .iter()
        .filter(|p| of(p.pid).0 == crate::categories::SYSTEM_GROUP)
        .map(|p| p.memory)
        .sum();
    let paths_known = snap.processes.iter().filter(|p| !p.path.is_empty()).count();
    r.check(
        "category grouping",
        tops.iter().map(|t| t.1).sum::<usize>() == snap.processes.len()
            && shown_procs == snap.processes.len()
            && tops.first().is_some_and(|t| t.0 == crate::categories::SYSTEM_GROUP && t.2 == macos_mem)
            && sections.len() >= 6
            && deepest_proc >= 3
            && of(1) == (crate::categories::SYSTEM_GROUP, "Core & Kernel".to_string())
            && window_server.as_ref().is_some_and(|c| c.1 == "Windows & Interface")
            && me.0 != crate::categories::SYSTEM_GROUP
            && misfiled_apps.is_empty()
            && native_under_launchd_not_macos == 0,
        format!(
            "groups {:?}; launchd -> {:?}; WindowServer -> {window_server:?}; this test -> {me:?}; {} app helpers misfiled; {native_under_launchd_not_macos} system daemons not in macOS; paths known for {paths_known}/{}; deepest process row depth {deepest_proc}",
            tops.iter().map(|t| format!("{} ({} procs, {})", t.0, t.1, crate::metrics::fmt_bytes(t.2))).collect::<Vec<_>>(),
            of(1),
            misfiled_apps.len(),
            snap.processes.len()
        ),
    );
    for (name, count, memory) in &sections {
        println!(
            "     macOS / {name}: {count} processes, {}",
            crate::metrics::fmt_bytes(*memory)
        );
    }
    if std::env::var_os("OTM_DEBUG").is_some() {
        let mut other: Vec<&str> = snap
            .processes
            .iter()
            .filter(|p| {
                of(p.pid)
                    == (
                        crate::categories::SYSTEM_GROUP,
                        crate::categories::OTHER_SYSTEM.to_string(),
                    )
            })
            .map(|p| p.name.as_str())
            .collect();
        other.sort();
        other.dedup();
        println!("     other system services: {}", other.join(", "));
    }
    for m in misfiled_apps.iter().take(5) {
        println!("     misfiled: {m}");
    }
    // Owner = the app a process belongs to; macOS's own processes are owned by "Operating System".
    let os = crate::categories::OS_OWNER;
    let owner = |name: &str| {
        snap.processes
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.owner_app.clone())
    };
    let siri: Vec<(String, String)> = snap
        .processes
        .iter()
        .filter(|p| p.name.to_lowercase().contains("siri") && p.path.starts_with("/System/"))
        .map(|p| (p.name.clone(), p.owner_app.clone()))
        .collect();
    let helpers_wrong = snap
        .processes
        .iter()
        .filter(|p| {
            p.path.contains(".app/")
                && !p.path.starts_with("/System/")
                && !p.path.starts_with("/Library/Apple/")
                && !p.path.contains("/Cryptexes/")
        })
        .filter(|p| {
            p.path.split('/').find_map(|s| s.strip_suffix(".app")) != Some(p.owner_app.as_str())
        })
        .count();
    let mine = snap
        .processes
        .iter()
        .find(|p| p.pid == std::process::id())
        .map(|p| p.owner_app.clone());
    layout.group_by = GroupBy::App;
    let owner_rows = build_rows(&snap.processes, &layout, "", &HashSet::new());
    let owner_groups: Vec<(String, usize, Option<&str>)> = owner_rows
        .iter()
        .filter_map(|row| match row {
            Row::Group {
                label, count, note, ..
            } => Some((label.clone(), *count, *note)),
            _ => None,
        })
        .collect();
    r.check(
        "owner app",
        snap.processes.iter().all(|p| !p.owner_app.is_empty())
            && owner("launchd").as_deref() == Some(os)
            && owner("WindowServer").as_deref() == Some(os)
            && !siri.is_empty()
            && siri.iter().all(|s| s.1 == os)
            && helpers_wrong == 0
            && mine.as_deref() != Some(os)
            && owner_groups.first().is_some_and(|g| g.0 == os && g.2 == Some("built into the system"))
            && owner_groups.iter().map(|g| g.1).sum::<usize>() == snap.processes.len(),
        format!(
            "launchd -> {:?}, WindowServer -> {:?}, Siri processes -> {:?}; {helpers_wrong} app helpers with the wrong owner; this test -> {mine:?}; owner groups: {:?}",
            owner("launchd"),
            owner("WindowServer"),
            siri.iter().map(|s| format!("{}: {}", s.0, s.1)).take(4).collect::<Vec<_>>(),
            owner_groups.iter().take(6).map(|g| format!("{} ({})", g.0, g.1)).collect::<Vec<_>>()
        ),
    );
    // Startup + services on a disposable launch agent.
    let plist = super::launchd::user_agents_dir().join(format!("{LABEL}.plist"));
    fs::write(
        &plist,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>Label</key><string>{LABEL}</string><key>ProgramArguments</key><array><string>/bin/sleep</string><string>3600</string></array><key>RunAtLoad</key><false/></dict></plist>"#
        ),
    )
    .unwrap();
    let res = sysops::toggle_startup(&plist, true);
    let listed = sysops::startup_items()
        .into_iter()
        .find(|s| s.label == LABEL);
    r.check(
        "startup enable",
        res.is_ok() && listed.as_ref().is_some_and(|s| s.loaded && s.editable),
        format!("{res:?}, listed loaded={:?}", listed.map(|s| s.loaded)),
    );
    let mut restarts = Vec::new();
    for _ in 0..3 {
        let res = sysops::control_service(LABEL, ServiceAction::Restart);
        sleep(Duration::from_millis(1500));
        let pid = service_pid();
        let up = pid.is_some_and(alive);
        let stop = sysops::control_service(LABEL, ServiceAction::Stop);
        sleep(Duration::from_millis(1500));
        let gone = pid.is_some_and(|p| !alive(p));
        restarts.push((res.is_ok() && up, stop.is_ok() && gone, pid));
    }
    r.check(
        "service restart/stop x3",
        restarts.iter().all(|(a, b, _)| *a && *b),
        format!("(started, stopped, pid): {restarts:?}"),
    );
    let res = sysops::toggle_startup(&plist, false);
    let still = sysops::services().iter().any(|s| s.label == LABEL);
    r.check(
        "startup disable",
        res.is_ok() && !still,
        format!("{res:?}, still loaded={still}"),
    );
    fs::remove_file(&plist).ok();
    let sys = sysops::toggle_startup(
        &PathBuf::from("/Library/LaunchDaemons/com.apple.fake.plist"),
        false,
    );
    r.check("refuse system agent", sys.is_err(), format!("{sys:?}"));
    let bad = sysops::control_service("a;rm -rf /", ServiceAction::Stop);
    r.check("refuse bad label", bad.is_err(), format!("{bad:?}"));
    // Uninstall a disposable app bundle from ~/Applications.
    let apps_dir = home.join("Applications");
    fs::create_dir_all(&apps_dir).ok();
    let app = apps_dir.join("OTM Disposable Test.app");
    fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    fs::write(
        app.join("Contents/Info.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleShortVersionString</key><string>9.9.9</string></dict></plist>"#,
    )
    .unwrap();
    let found = sysops::installed_apps().into_iter().find(|a| a.path == app);
    r.check(
        "apps list",
        found.as_ref().is_some_and(|a| a.version == "9.9.9"),
        format!("version {:?}", found.map(|a| a.version)),
    );
    let res = sysops::trash_app(&app);
    let in_trash = home
        .join(".Trash")
        .read_dir()
        .map(|d| {
            d.flatten().any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("OTM Disposable Test")
            })
        })
        .unwrap_or(false);
    r.check(
        "uninstall to Trash",
        res.is_ok() && !app.exists(),
        format!(
            "{res:?}, removed={}, seen in ~/.Trash={in_trash}",
            !app.exists()
        ),
    );
    let outside = sysops::trash_app(&PathBuf::from("/System/Applications/Calculator.app"));
    r.check(
        "refuse outside /Applications",
        outside.is_err(),
        format!("{outside:?}"),
    );

    // Connections, power, recorder round-trip.
    let conns = sysops::connections();
    r.check(
        "connections",
        !conns.is_empty(),
        format!(
            "{} sockets, e.g. {:?}",
            conns.len(),
            conns.first().map(|c| (&c.process, &c.proto, &c.endpoint))
        ),
    );
}
