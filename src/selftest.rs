//! `otm --selftest`: exercises the real action code paths on disposable targets and prints PASS/FAIL.

use std::{
    collections::HashSet, fs, path::PathBuf, process::Command, thread::sleep, time::Duration,
};

use crate::{
    columns::{ColumnId, GroupBy, TableLayout},
    metrics::Sampler,
    processes::{Row, build_rows},
    recorder::{self, Frame},
    sysops::{self, ServiceAction},
};

const LABEL: &str = "test.otm.native.disposable";

struct Report {
    failed: usize,
}

impl Report {
    fn check(&mut self, name: &str, ok: bool, detail: impl AsRef<str>) {
        if !ok {
            self.failed += 1;
        }
        println!(
            "{} {name}: {}",
            if ok { "PASS" } else { "FAIL" },
            detail.as_ref()
        );
    }
}

fn service_pid() -> Option<u32> {
    sysops::services()
        .into_iter()
        .find(|s| s.label == LABEL)
        .and_then(|s| s.pid)
}

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

pub fn run() -> i32 {
    let mut r = Report { failed: 0 };
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());

    // Metrics and grouping by owner.
    let mut sampler = Sampler::new();
    sampler.sample();
    sleep(Duration::from_millis(500));
    let snap = sampler.sample();
    let owners: HashSet<&str> = snap.processes.iter().map(|p| p.user.as_str()).collect();
    r.check(
        "sample",
        snap.processes.len() > 50 && snap.memory_total > 0,
        format!(
            "{} processes, {} owners, cpu {:.1}%",
            snap.processes.len(),
            owners.len(),
            snap.cpu
        ),
    );
    let mut layout = TableLayout {
        group_by: GroupBy::User,
        ..Default::default()
    };
    let rows = build_rows(&snap.processes, &layout, "", &HashSet::new());
    let groups: Vec<(String, usize)> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Group { label, count, .. } => Some((label.clone(), *count)),
            _ => None,
        })
        .collect();
    let grouped_total: usize = groups.iter().map(|g| g.1).sum();
    r.check(
        "group by owner",
        groups.len() == owners.len() && grouped_total == snap.processes.len(),
        format!(
            "{} groups covering {grouped_total} processes: {:?}",
            groups.len(),
            &groups[..groups.len().min(4)]
        ),
    );
    let unknown = snap
        .processes
        .iter()
        .filter(|p| p.user == "unknown")
        .count();
    r.check(
        "owners resolved",
        unknown * 50 <= snap.processes.len(),
        format!(
            "{unknown} of {} processes without a known owner",
            snap.processes.len()
        ),
    );
    let root_mem: u64 = snap
        .processes
        .iter()
        .filter(|p| p.user == "root")
        .map(|p| p.memory)
        .sum();
    let zero_mem = snap.processes.iter().filter(|p| p.memory == 0).count();
    r.check(
        "other users' memory",
        root_mem > 200 * 1024 * 1024 && zero_mem * 20 <= snap.processes.len(),
        format!(
            "root processes total {}, {zero_mem} processes with 0 memory",
            crate::metrics::fmt_bytes(root_mem)
        ),
    );
    let first = groups.first().map(|g| g.0.clone()).unwrap_or_default();
    let collapsed: HashSet<String> = [first.clone()].into();
    let rows_collapsed = build_rows(&snap.processes, &layout, "", &collapsed);
    r.check(
        "collapse group",
        rows_collapsed.len() + groups[0].1 == rows.len(),
        format!(
            "collapsing '{first}' hides {} rows",
            rows.len() - rows_collapsed.len()
        ),
    );
    // Process tree: every indented row sits directly under its parent, launchd is a root.
    layout.group_by = GroupBy::Parent;
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
    // The parent of a row is the nearest process row above it with a smaller depth
    // (section headers such as "Operating System" may sit in between).
    let mut path: Vec<(usize, u32)> = Vec::new();
    let mut misplaced = 0;
    for &(pid, ppid, depth, _) in &nodes {
        while path.last().is_some_and(|&(d, _)| d >= depth) {
            path.pop();
        }
        if depth > 0 && path.last().map(|&(_, p)| p) != ppid {
            misplaced += 1;
        }
        path.push((depth, pid));
    }
    let max_depth = nodes.iter().map(|n| n.2).max().unwrap_or(0);
    let roots = nodes.iter().filter(|n| n.2 == 0).count();
    let launchd = nodes.iter().find(|n| n.0 == 1);
    r.check(
        "process tree",
        nodes.len() == snap.processes.len() && misplaced == 0 && max_depth >= 2 && roots <= 3 && launchd.is_some_and(|n| n.2 == 0 && n.3 + 3 >= nodes.len()),
        format!(
            "{} rows, {roots} roots, max depth {max_depth}, launchd has {} descendants, {misplaced} rows not under their parent",
            nodes.len(),
            launchd.map(|n| n.3).unwrap_or(0)
        ),
    );
    // Memory column in the tree: each branch = its own memory + its children's branch totals.
    let tree_mem: std::collections::HashMap<u32, (u64, u64, Option<u32>)> = tree
        .iter()
        .filter_map(|row| match row {
            Row::Process {
                info, tree_memory, ..
            } => Some((info.pid, (info.memory, *tree_memory, info.ppid))),
            _ => None,
        })
        .collect();
    let mut child_sum: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    for (&pid, &(_, total, ppid)) in &tree_mem {
        if let Some(pp) = ppid.filter(|&pp| pp != pid && tree_mem.contains_key(&pp)) {
            *child_sum.entry(pp).or_default() += total;
        }
    }
    let wrong = tree_mem
        .iter()
        .filter(|(pid, (own, total, _))| *total != own + child_sum.get(pid).copied().unwrap_or(0))
        .count();
    let all_mem: u64 = snap.processes.iter().map(|p| p.memory).sum();
    let launchd_total = tree_mem.get(&1).map(|t| t.1).unwrap_or(0);
    let launchd_own = tree_mem.get(&1).map(|t| t.0).unwrap_or(0);
    let biggest = tree
        .iter()
        .filter_map(|row| match row {
            Row::Process {
                info,
                depth: 1,
                tree_memory,
                descendants,
                ..
            } if *descendants > 0 => {
                Some((info.name.clone(), *descendants, *tree_memory, info.memory))
            }
            _ => None,
        })
        .max_by_key(|b| b.2);
    r.check(
        "memory sums branches",
        wrong == 0 && launchd_total + 2 * 1024 * 1024 * 1024 >= all_mem && launchd_total > launchd_own,
        format!(
            "{wrong} branches whose total != own + children; launchd own {} vs branch {} (all processes {}); biggest app branch {:?}",
            crate::metrics::fmt_bytes(launchd_own),
            crate::metrics::fmt_bytes(launchd_total),
            crate::metrics::fmt_bytes(all_mem),
            biggest.map(|b| format!("{} + {} children = {} (own {})", b.0, b.1, crate::metrics::fmt_bytes(b.2), crate::metrics::fmt_bytes(b.3)))
        ),
    );
    let filtered_launchd = build_rows(&snap.processes, &layout, "otm", &HashSet::new())
        .iter()
        .find_map(|row| match row {
            Row::Process {
                info, tree_memory, ..
            } if info.pid == 1 => Some(*tree_memory),
            _ => None,
        })
        .unwrap_or(0);
    r.check(
        "filter keeps full sums",
        filtered_launchd == launchd_total
            && crate::processes::memory_text(10, 3 * 1024 * 1024 * 1024, 4, true) == "Σ 3.0 GB"
            && crate::processes::memory_text(10 * 1024 * 1024, 99, 0, true) == "10.0 MB",
        format!("launchd branch with filter \"otm\": {} (same as unfiltered); cells read \"Σ 3.0 GB\" for a branch, own value for a leaf", crate::metrics::fmt_bytes(filtered_launchd)),
    );
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
    let folded = build_rows(
        &snap.processes,
        &layout,
        "",
        &[crate::processes::node_key(1)].into(),
    );
    r.check(
        "collapse tree branch",
        folded.len() == nodes.len() - launchd.map(|n| n.3).unwrap_or(0),
        format!(
            "collapsing launchd hides its {} descendants: {} -> {} rows",
            launchd.map(|n| n.3).unwrap_or(0),
            nodes.len(),
            folded.len()
        ),
    );
    let me = std::process::id();
    let filtered = build_rows(
        &snap.processes,
        &layout,
        "otm",
        &[crate::processes::node_key(1)].into(),
    );
    let shown: Vec<u32> = filtered
        .iter()
        .filter_map(|row| match row {
            Row::Process { info, .. } => Some(info.pid),
            _ => None,
        })
        .collect();
    r.check(
        "filter keeps ancestors",
        shown.contains(&me) && shown.contains(&1),
        format!("filter \"otm\" shows {} rows incl. this process {me} and launchd, even with launchd collapsed", shown.len()),
    );
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
            } if current_top == crate::categories::MACOS => {
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
        .filter(|p| of(p.pid).0 != crate::categories::MACOS)
        .count();
    let macos_mem: u64 = snap
        .processes
        .iter()
        .filter(|p| of(p.pid).0 == crate::categories::MACOS)
        .map(|p| p.memory)
        .sum();
    let paths_known = snap.processes.iter().filter(|p| !p.path.is_empty()).count();
    r.check(
        "category grouping",
        tops.iter().map(|t| t.1).sum::<usize>() == snap.processes.len()
            && shown_procs == snap.processes.len()
            && tops.first().is_some_and(|t| t.0 == crate::categories::MACOS && t.2 == macos_mem)
            && sections.len() >= 6
            && deepest_proc >= 3
            && of(1) == (crate::categories::MACOS, "Core & Kernel".to_string())
            && window_server.as_ref().is_some_and(|c| c.1 == "Windows & Interface")
            && me.0 != crate::categories::MACOS
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
                        crate::categories::MACOS,
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

    layout.group_by = GroupBy::User;
    let owner_tree = build_rows(&snap.processes, &layout, "", &HashSet::new());
    let nested = owner_tree
        .iter()
        .any(|row| matches!(row, Row::Process { depth, .. } if *depth >= 2));
    r.check(
        "owner groups hold trees",
        nested,
        "rows nested at depth 2+ inside owner groups",
    );

    // Column layout: hide, reorder, resize, persist round-trip.
    let mut l = TableLayout::default();
    l.toggle(ColumnId::User);
    l.move_column(ColumnId::Memory, ColumnId::Name);
    l.set_width(0, 333.);
    l.sort_by(ColumnId::Memory);
    let json = serde_json::to_string(&l).unwrap();
    let back: TableLayout = serde_json::from_str(&json).unwrap();
    let order: Vec<ColumnId> = back.visible().map(|(_, c)| c.id).collect();
    r.check(
        "columns",
        !order.contains(&ColumnId::User)
            && order[0] == ColumnId::Memory
            && back.columns[0].width == 333.
            && back.sort == ColumnId::Memory,
        format!(
            "visible order after hide/move/resize/save/load: {order:?}, first width {}",
            back.columns[0].width
        ),
    );
    let mut only = TableLayout::default();
    for c in ColumnId::ALL {
        only.toggle(c);
    }
    r.check(
        "last column stays visible",
        only.visible().count() >= 1,
        format!("{} visible", only.visible().count()),
    );

    // Startup + services on a disposable launch agent.
    let plist = sysops::user_agents_dir().join(format!("{LABEL}.plist"));
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

    // Quit / force quit a disposable process.
    let mut child = Command::new("/bin/sleep").arg("600").spawn().unwrap();
    let pid = child.id();
    let res = sysops::signal_process(pid, false);
    let status = child.wait().ok();
    r.check(
        "quit process",
        res.is_ok() && status.is_some_and(|s| !s.success()),
        format!("pid {pid}: {res:?}, exit {status:?}"),
    );
    let mut child = Command::new("/bin/sleep").arg("600").spawn().unwrap();
    let pid = child.id();
    let res = sysops::signal_process(pid, true);
    let status = child.wait().ok();
    r.check(
        "force quit process",
        res.is_ok() && status.is_some(),
        format!("pid {pid}: {res:?}, exit {status:?}"),
    );
    r.check(
        "refuse pid 1",
        sysops::signal_process(1, true).is_err(),
        "launchd protected",
    );

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
    let p = sysops::power();
    r.check(
        "power",
        p.percent.is_some() || !p.has_battery,
        format!(
            "{:?}% {} cycles {:?} health {:?} thermal {}",
            p.percent,
            p.source,
            p.cycles,
            p.health.map(|h| h.round()),
            p.thermal
        ),
    );
    let frames = vec![
        Frame::from_snapshot(&snap),
        Frame::from_snapshot(&sampler.sample()),
    ];
    let saved = recorder::save(frames);
    let loaded = saved.as_ref().ok().and_then(|p| recorder::load(p).ok());
    r.check(
        "recorder save/load",
        loaded
            .as_ref()
            .is_some_and(|l| l.frames.len() == 2 && l.format == recorder::FORMAT),
        format!("{saved:?}"),
    );
    if let Ok(p) = saved {
        fs::remove_file(p).ok();
    }

    let info = sysops::system_info();
    r.check(
        "system info",
        !info.model.is_empty() && info.memory > 0 && info.physical_cores > 0 && info.booted > 0,
        format!(
            "{} · {} · {} ({}) · {} cores ({}P/{}E) · {}",
            info.model,
            info.chip,
            info.os,
            info.build,
            info.physical_cores,
            info.performance_cores,
            info.efficiency_cores,
            crate::metrics::fmt_bytes(info.memory)
        ),
    );
    let skins = crate::settings::skins();
    r.check(
        "popout skins",
        skins.iter().all(|s| {
            crate::settings::Skin::color(&s.background, 0xff) != 0 || s.background == "000000"
        }),
        format!(
            "{} skin files: {:?}",
            skins.len(),
            skins.iter().map(|s| &s.name).collect::<Vec<_>>()
        ),
    );
    let settings = crate::settings::Settings {
        text_scale: 130,
        high_contrast: true,
        ..Default::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    let back: crate::settings::Settings = serde_json::from_str(&json).unwrap();
    let old: crate::settings::Settings = serde_json::from_str(
        r#"{"appearance":"Dark","glass":"Clear","refresh_secs":2,"mark_self":true}"#,
    )
    .unwrap();
    r.check(
        "settings round-trip",
        back.text_scale == 130
            && back.high_contrast
            && old.text_scale == 100
            && old.refresh_secs == 2,
        "text size and contrast persist; older settings files still load with defaults",
    );

    if std::env::args().any(|a| a == "--scan") {
        let before = sysops::load_scans().len();
        let scans = sysops::scan_home();
        let top = scans.first().and_then(|s| s.items.first()).cloned();
        r.check(
            "disk scan + history",
            scans.len() == (before + 1).min(10) && top.is_some(),
            format!(
                "history {before} -> {}, largest {:?}",
                scans.len(),
                top.map(|t| (t.path, crate::metrics::fmt_bytes(t.bytes)))
            ),
        );
    }

    if std::env::args().any(|a| a == "--bench") {
        let b = sysops::run_benchmarks();
        r.check(
            "benchmarks",
            b.multi_core > b.single_core && b.disk_write_mbps > 0. && b.disk_read_mbps > 0.,
            format!(
                "single {:.0} Mops/s, all {} threads {:.0} Mops/s, memory {:.1} GB/s, disk write {:.0} MB/s, read {:.0} MB/s",
                b.single_core, b.threads, b.multi_core, b.memory_gbps, b.disk_write_mbps, b.disk_read_mbps
            ),
        );
    }

    println!("{} checks failed", r.failed);
    if r.failed == 0 { 0 } else { 1 }
}
