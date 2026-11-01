//! `otm --selftest`: exercises the real action code paths on disposable targets and prints PASS/FAIL.

use std::{collections::HashSet, fs, process::Command, thread::sleep, time::Duration};

use crate::{
    columns::{ColumnId, GroupBy, TableLayout},
    metrics::Sampler,
    processes::{Row, build_rows},
    recorder::{self, Frame},
    sysops,
};

pub(crate) struct Report {
    pub(crate) failed: usize,
}

impl Report {
    pub(crate) fn check(&mut self, name: &str, ok: bool, detail: impl AsRef<str>) {
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

pub fn run() -> i32 {
    let mut r = Report { failed: 0 };

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
    let zero_mem = snap
        .processes
        .iter()
        .filter(|p| crate::platform::selftest::unmeasured(p))
        .count();
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
    // Linux files kernel threads under kthreadd (PID 2), beside init; macOS has none.
    let kernel = nodes
        .iter()
        .find(|n| n.0 == 2 && n.2 == 0)
        .map_or(0, |n| n.3 + 1);
    r.check(
        "process tree",
        nodes.len() == snap.processes.len() && misplaced == 0 && max_depth >= 2 && roots <= 3 && launchd.is_some_and(|n| n.2 == 0 && crate::platform::selftest::init_holds_tree(n.3, kernel, nodes.len())),
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

    crate::platform::selftest::app_checks(&mut r, &snap);

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
    // A filter starts with every branch open (its own fold state, see `OtmApp::folds`).
    let filtered = build_rows(&snap.processes, &layout, "otm", &HashSet::new());
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
        format!(
            "filter \"otm\" shows {} rows incl. this process {me} and launchd",
            shown.len()
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
