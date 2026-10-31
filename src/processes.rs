use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    rc::Rc,
};

use gpui::{
    AnyElement, Context, FontWeight, MouseButton, MouseDownEvent, Render, ScrollStrategy, Window,
    anchored, deferred, div, img, prelude::*, px, uniform_list,
};

use crate::{
    app::{OtmApp, Page, Resize},
    columns::{ColumnId, GroupBy, TableLayout},
    filter::Filter,
    metrics::{ProcessInfo, fmt_bytes},
    theme::Theme,
    ui,
};

const ROW_HEIGHT: f32 = 26.;

pub enum Row {
    Group {
        /// Indent level: 0 for a top group, 1 for a section inside it.
        depth: usize,
        /// Shown after the label, e.g. "built into the system".
        note: Option<&'static str>,
        key: String,
        label: String,
        count: usize,
        cpu: f32,
        memory: u64,
        collapsed: bool,
    },
    Process {
        info: ProcessInfo,
        /// Indent level: 0 for a tree root (or a flat row), +1 per ancestor shown above it.
        depth: usize,
        /// Number of descendants under this process (0 = leaf).
        descendants: usize,
        collapsed: bool,
        /// Memory of this process plus all of its descendants (its own memory in the flat list).
        tree_memory: u64,
    },
}

#[derive(Clone)]
pub struct ColumnDrag {
    id: ColumnId,
}

impl Render for ColumnDrag {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::for_appearance(window.appearance());
        div()
            .px_3()
            .py_1()
            .rounded_full()
            .bg(t.glass_strong)
            .border_1()
            .border_color(t.accent)
            .text_xs()
            .text_color(t.text)
            .shadow_lg()
            .child(self.id.title())
    }
}

/// The Memory cell: in the tree views a branch shows its total (itself plus everything under it),
/// marked with "Σ"; a leaf, or any row in the flat list, shows its own memory.
pub fn memory_text(own: u64, tree_memory: u64, descendants: usize, tree: bool) -> String {
    if tree && descendants > 0 {
        format!("Σ {}", fmt_bytes(tree_memory))
    } else {
        fmt_bytes(own)
    }
}

/// Key in `collapsed` for a process node in a tree.
pub fn node_key(pid: u32) -> String {
    format!("pid:{pid}")
}

/// What a branch adds up to: the process itself plus everything under it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Totals {
    pub descendants: usize,
    pub cpu: f32,
    pub memory: u64,
}

/// Branch totals for every process in `universe`, following parent links inside `universe` only.
/// Computed iteratively in post-order; parent cycles are cut, so nothing is counted twice.
pub fn subtree_totals(universe: &[ProcessInfo]) -> HashMap<u32, Totals> {
    let index: HashMap<u32, usize> = universe
        .iter()
        .enumerate()
        .map(|(i, p)| (p.pid, i))
        .collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); universe.len()];
    let mut roots = Vec::new();
    for (i, p) in universe.iter().enumerate() {
        match p.ppid.and_then(|pp| index.get(&pp)) {
            Some(&parent) if parent != i => children[parent].push(i),
            _ => roots.push(i),
        }
    }
    let mut total = vec![Totals::default(); universe.len()];
    let mut seen = vec![false; universe.len()];
    // A child counts toward its parent once its own total is final; this is what cuts cycles.
    let mut done = vec![false; universe.len()];
    let mut stack: Vec<(usize, bool)> = roots.iter().map(|&r| (r, false)).collect();
    // Anything only reachable through a cycle is started as its own root afterwards.
    let mut extra = (0..universe.len()).rev().collect::<Vec<_>>();
    loop {
        while let Some((i, finish)) = stack.pop() {
            if finish {
                let mut t = Totals {
                    descendants: 0,
                    cpu: universe[i].cpu,
                    memory: universe[i].memory,
                };
                for &c in &children[i] {
                    if done[c] {
                        t.descendants += 1 + total[c].descendants;
                        t.cpu += total[c].cpu;
                        t.memory += total[c].memory;
                    }
                }
                total[i] = t;
                done[i] = true;
            } else if !seen[i] {
                seen[i] = true;
                stack.push((i, true));
                stack.extend(
                    children[i]
                        .iter()
                        .filter(|&&c| !seen[c])
                        .map(|&c| (c, false)),
                );
            }
        }
        match extra.iter().position(|&i| !seen[i]) {
            Some(k) => {
                let i = extra[k];
                extra.truncate(k);
                stack.push((i, false));
            }
            None => break,
        }
    }
    universe
        .iter()
        .zip(total)
        .map(|(p, t)| (p.pid, t))
        .collect()
}

/// Lay `list` out as a parent/child tree. A process whose parent is not in `list` (or is itself)
/// becomes a root. Siblings are ordered by the sort column; for CPU and Memory a branch ranks by
/// its whole subtree (from `totals`), so a busy app and its helpers rise together.
#[allow(clippy::too_many_arguments)]
fn tree_rows(
    list: Vec<ProcessInfo>,
    totals: &HashMap<u32, Totals>,
    layout: &TableLayout,
    base_depth: usize,
    collapsed: &HashSet<String>,
    expand_all: bool,
    split: Option<&Split>,
    rows: &mut Vec<Row>,
) {
    let index: HashMap<u32, usize> = list.iter().enumerate().map(|(i, p)| (p.pid, i)).collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); list.len()];
    let mut roots = Vec::new();
    for (i, p) in list.iter().enumerate() {
        match p.ppid.and_then(|pp| index.get(&pp)) {
            Some(&parent) if parent != i => children[parent].push(i),
            _ => roots.push(i),
        }
    }
    let t = |i: usize| totals.get(&list[i].pid).copied().unwrap_or_default();

    let sort = layout.sort;
    let desc = layout.descending;
    let order = |a: &usize, b: &usize| {
        let o = match sort {
            ColumnId::Cpu => t(*a).cpu.total_cmp(&t(*b).cpu),
            ColumnId::Memory => t(*a).memory.cmp(&t(*b).memory),
            _ => sort.compare(&list[*a], &list[*b]),
        };
        if desc { o.reverse() } else { o }
    };
    // Members of a parent cycle are never reached from a root (folding aside); make them roots too.
    let mut reached = vec![false; list.len()];
    let mut walk: Vec<usize> = roots.clone();
    while let Some(i) = walk.pop() {
        if !reached[i] {
            reached[i] = true;
            walk.extend(children[i].iter().copied());
        }
    }
    for i in 0..list.len() {
        if !reached[i] {
            roots.push(i);
            let mut walk = vec![i];
            while let Some(j) = walk.pop() {
                if !reached[j] {
                    reached[j] = true;
                    walk.extend(children[j].iter().copied());
                }
            }
        }
    }
    roots.sort_by(order);
    for c in &mut children {
        c.sort_by(order);
    }

    enum Item {
        Node(usize, usize),
        Section {
            part: usize,
            depth: usize,
            members: Vec<usize>,
        },
        Sub {
            part: usize,
            name: String,
            depth: usize,
            members: Vec<usize>,
        },
    }
    let mut emitted = vec![false; list.len()];
    let mut stack: Vec<Item> = roots
        .iter()
        .rev()
        .map(|&r| Item::Node(r, base_depth))
        .collect();
    while let Some(item) = stack.pop() {
        match item {
            Item::Section {
                part,
                depth,
                members,
            } => {
                let (label, note) = SPLIT_SECTIONS[part];
                let key = format!("split:{part}");
                let (count, cpu, memory) = split.map(|s| s.totals[part]).unwrap_or_default();
                let folded = !expand_all && collapsed.contains(&key);
                rows.push(Row::Group {
                    depth,
                    note,
                    label: label.to_string(),
                    count,
                    cpu,
                    memory,
                    collapsed: folded,
                    key,
                });
                if !folded {
                    // One node per macOS subsystem (OS section) or per owning app (yours).
                    let mut subs: Vec<(String, Vec<usize>)> = Vec::new();
                    for c in members {
                        let name = split
                            .and_then(|s| s.place.get(&list[c].pid))
                            .map(|p| p.1.clone())
                            .unwrap_or_else(|| list[c].owner_app.clone());
                        match subs.iter_mut().find(|s| s.0 == name) {
                            Some(s) => s.1.push(c),
                            None => subs.push((name, vec![c])),
                        }
                    }
                    let sub_total = |name: &str| {
                        split
                            .and_then(|s| s.sub_totals.get(&(part, name.to_string())).copied())
                            .unwrap_or_default()
                    };
                    subs.sort_by(|a, b| {
                        let (ta, tb) = (sub_total(&a.0), sub_total(&b.0));
                        let o = match sort {
                            ColumnId::Memory | ColumnId::OwnMemory => ta.2.cmp(&tb.2),
                            ColumnId::Cpu => ta.1.total_cmp(&tb.1),
                            _ => b.0.to_lowercase().cmp(&a.0.to_lowercase()),
                        };
                        if desc { o.reverse() } else { o }
                    });
                    stack.extend(subs.into_iter().rev().map(|(name, members)| Item::Sub {
                        part,
                        name,
                        depth: depth + 1,
                        members,
                    }));
                }
            }
            Item::Sub {
                part,
                name,
                depth,
                members,
            } => {
                let key = format!("split:{part}/{name}");
                let (count, cpu, memory) = split
                    .and_then(|s| s.sub_totals.get(&(part, name.clone())).copied())
                    .unwrap_or_default();
                let folded = !expand_all && collapsed.contains(&key);
                rows.push(Row::Group {
                    depth,
                    note: None,
                    label: name,
                    count,
                    cpu,
                    memory,
                    collapsed: folded,
                    key,
                });
                if !folded {
                    stack.extend(members.into_iter().rev().map(|c| Item::Node(c, depth + 1)));
                }
            }
            Item::Node(i, depth) => {
                if emitted[i] {
                    continue;
                }
                emitted[i] = true;
                let is_collapsed = !expand_all && collapsed.contains(&node_key(list[i].pid));
                let total = t(i);
                rows.push(Row::Process {
                    info: list[i].clone(),
                    depth,
                    descendants: total.descendants,
                    collapsed: is_collapsed && !children[i].is_empty(),
                    tree_memory: total.memory,
                });
                if is_collapsed {
                    continue;
                }
                if split.is_some() && list[i].pid == 1 {
                    // launchd: split its children into the operating system's and yours.
                    let (os, yours): (Vec<usize>, Vec<usize>) = children[i]
                        .iter()
                        .partition(|&&c| list[c].owner_app == crate::categories::OS_OWNER);
                    for (part, members) in [(1, yours), (0, os)] {
                        if !members.is_empty() {
                            stack.push(Item::Section {
                                part,
                                depth: depth + 1,
                                members,
                            });
                        }
                    }
                } else {
                    stack.extend(children[i].iter().rev().map(|&c| Item::Node(c, depth + 1)));
                }
            }
        }
    }
}

/// The two sections under launchd in the Tree view: (label, note).
const SPLIT_SECTIONS: [(&str, Option<&str>); 2] = [
    ("Operating System", Some("built into the system")),
    ("Your Processes", Some("apps and tools you run")),
];

/// How launchd's children are laid out in the Tree view: two sections (the operating system's
/// and yours), and inside them one node per macOS subsystem / per owning app.
pub struct Split {
    /// (count, CPU, memory) of each section over the whole process list.
    pub totals: [(usize, f32, u64); 2],
    /// For each child of launchd: (section 0/1, sub-group name).
    pub place: HashMap<u32, (usize, String)>,
    /// (count, CPU, memory) of each (section, sub-group).
    pub sub_totals: HashMap<(usize, String), (usize, f32, u64)>,
}

/// Section and sub-group of a direct child of launchd: macOS processes go to their subsystem,
/// everything else to the app that owns it.
fn split_place(p: &ProcessInfo, cats: &HashMap<u32, (&'static str, String)>) -> (usize, String) {
    if p.owner_app == crate::categories::OS_OWNER {
        (
            0,
            cats.get(&p.pid)
                .map(|c| c.1.clone())
                .unwrap_or_else(|| crate::categories::OTHER_SYSTEM.to_string()),
        )
    } else {
        (1, p.owner_app.clone())
    }
}

fn split_info(processes: &[ProcessInfo], totals: &HashMap<u32, Totals>) -> Split {
    let cats = crate::categories::categorize(processes);
    let mut split = Split {
        totals: [(0, 0., 0); 2],
        place: HashMap::new(),
        sub_totals: HashMap::new(),
    };
    for p in processes.iter().filter(|p| p.ppid == Some(1) && p.pid != 1) {
        let (part, sub) = split_place(p, &cats);
        let t = totals.get(&p.pid).copied().unwrap_or_default();
        for slot in [
            &mut split.totals[part],
            split.sub_totals.entry((part, sub.clone())).or_default(),
        ] {
            slot.0 += 1 + t.descendants;
            slot.1 += t.cpu;
            slot.2 += t.memory;
        }
        split.place.insert(p.pid, (part, sub));
    }
    split
}

/// Which section (and sub-group) of launchd's tree a process lives in: found from the child of
/// launchd it descends from. None for launchd itself and processes outside its tree.
fn split_branch(p: &ProcessInfo, by_pid: &HashMap<u32, &ProcessInfo>) -> Option<u32> {
    let mut cur = p;
    for _ in 0..64 {
        match cur.ppid {
            Some(1) if cur.pid != 1 => return Some(cur.pid),
            Some(pp) if pp != cur.pid => cur = by_pid.get(&pp)?,
            _ => return None,
        }
    }
    None
}

/// Filter, sort and group processes into table rows: a flat list, a process tree, or owner
/// groups that each contain their own process tree.
pub fn build_rows(
    processes: &[ProcessInfo],
    layout: &TableLayout,
    query: &str,
    collapsed: &HashSet<String>,
) -> Vec<Row> {
    build_rows_filtered(
        processes,
        layout,
        &Filter::new(&[], query, processes),
        collapsed,
    )
}

/// [`build_rows`] with a full filter: free text plus chips.
pub fn build_rows_filtered(
    processes: &[ProcessInfo],
    layout: &TableLayout,
    filter: &Filter,
    collapsed: &HashSet<String>,
) -> Vec<Row> {
    if layout.group_by == GroupBy::None {
        let mut list: Vec<ProcessInfo> = processes
            .iter()
            .filter(|p| filter.matches(p))
            .cloned()
            .collect();
        let (sort, desc) = (layout.sort, layout.descending);
        list.sort_by(|a, b| {
            let o = sort.compare(a, b);
            if desc { o.reverse() } else { o }
        });
        return list
            .into_iter()
            .map(|info| {
                let tree_memory = info.memory;
                Row::Process {
                    info,
                    depth: 0,
                    descendants: 0,
                    collapsed: false,
                    tree_memory,
                }
            })
            .collect();
    }

    // Keep the matches plus every ancestor, so each match is shown in place in the tree.
    let by_pid: HashMap<u32, &ProcessInfo> = processes.iter().map(|p| (p.pid, p)).collect();
    let mut keep: HashSet<u32> = HashSet::new();
    for p in processes.iter().filter(|p| filter.matches(p)) {
        let mut cur = Some(p);
        while let Some(c) = cur {
            if !keep.insert(c.pid) {
                break;
            }
            cur = c
                .ppid
                .filter(|&pp| pp != c.pid)
                .and_then(|pp| by_pid.get(&pp).copied());
        }
    }
    let list: Vec<ProcessInfo> = processes
        .iter()
        .filter(|p| keep.contains(&p.pid))
        .cloned()
        .collect();
    let expand_all = filter.is_active();
    let mut rows = Vec::new();

    if layout.group_by == GroupBy::Category {
        category_rows(processes, list, layout, collapsed, expand_all, &mut rows);
        return rows;
    }

    if layout.group_by == GroupBy::Parent {
        // Totals cover the whole tree, so filtering or folding never shrinks a branch's sum.
        let totals = subtree_totals(processes);
        let split = split_info(processes, &totals);
        tree_rows(
            list,
            &totals,
            layout,
            0,
            collapsed,
            expand_all,
            Some(&split),
            &mut rows,
        );
        return rows;
    }

    // Owner groups (the app a process belongs to, "Operating System" for macOS's own) or user
    // account groups, each holding a tree. Sums cover all of the group's processes, not just the
    // ones the filter shows.
    let by_app = layout.group_by == GroupBy::App;
    let key_of = |p: &ProcessInfo| {
        if by_app {
            p.owner_app.clone()
        } else {
            p.user.clone()
        }
    };
    let mut everyone: HashMap<String, Vec<ProcessInfo>> = HashMap::new();
    for p in processes {
        everyone.entry(key_of(p)).or_default().push(p.clone());
    }
    let mut groups: HashMap<String, Vec<ProcessInfo>> = HashMap::new();
    for p in list {
        groups.entry(key_of(&p)).or_default().push(p);
    }
    let mut order: Vec<(String, f32, u64)> = groups
        .keys()
        .map(|k| {
            let all = &everyone[k];
            (
                k.clone(),
                all.iter().map(|p| p.cpu).sum(),
                all.iter().map(|p| p.memory).sum(),
            )
        })
        .collect();
    let (sort, desc) = (layout.sort, layout.descending);
    order.sort_by(|a, b| {
        let o = match sort {
            ColumnId::Memory | ColumnId::OwnMemory => a.2.cmp(&b.2),
            ColumnId::Cpu => a.1.total_cmp(&b.1),
            _ => b.0.to_lowercase().cmp(&a.0.to_lowercase()),
        };
        if desc { o.reverse() } else { o }
    });
    if by_app {
        // macOS's own processes always come first.
        if let Some(i) = order
            .iter()
            .position(|o| o.0 == crate::categories::OS_OWNER)
        {
            let os = order.remove(i);
            order.insert(0, os);
        }
    }
    for (key, cpu, memory) in order {
        let items = groups.remove(&key).unwrap_or_default();
        let is_collapsed = !expand_all && collapsed.contains(&key);
        let totals = (!is_collapsed).then(|| subtree_totals(&everyone[&key]));
        let note =
            (by_app && key == crate::categories::OS_OWNER).then_some("built into the system");
        let count = everyone[&key].len();
        rows.push(Row::Group {
            depth: 0,
            note,
            label: key.clone(),
            count,
            cpu,
            memory,
            collapsed: is_collapsed,
            key,
        });
        if let Some(totals) = totals {
            tree_rows(
                items, &totals, layout, 1, collapsed, expand_all, None, &mut rows,
            );
        }
    }
    rows
}

/// Category grouping: macOS (split into related subsystems) / Apps (one section per app) /
/// Command-line & Other, each section holding its own process tree. Sums cover every process in
/// the group, not just what the filter shows.
fn category_rows(
    processes: &[ProcessInfo],
    list: Vec<ProcessInfo>,
    layout: &TableLayout,
    collapsed: &HashSet<String>,
    expand_all: bool,
    rows: &mut Vec<Row>,
) {
    use crate::categories::{MACOS, TOPS, categorize};
    let cat = categorize(processes);
    let section_of = |p: &ProcessInfo| cat.get(&p.pid).cloned().unwrap_or((MACOS, String::new()));
    // top -> section -> processes (everything, for sums; shown, for rows)
    let mut all: HashMap<&'static str, HashMap<String, Vec<ProcessInfo>>> = HashMap::new();
    for p in processes {
        let (top, sec) = section_of(p);
        all.entry(top)
            .or_default()
            .entry(sec)
            .or_default()
            .push(p.clone());
    }
    let mut shown: HashMap<&'static str, HashMap<String, Vec<ProcessInfo>>> = HashMap::new();
    for p in list {
        let (top, sec) = section_of(&p);
        shown
            .entry(top)
            .or_default()
            .entry(sec)
            .or_default()
            .push(p);
    }
    let sums = |v: &[ProcessInfo]| {
        (
            v.iter().map(|p| p.cpu).sum::<f32>(),
            v.iter().map(|p| p.memory).sum::<u64>(),
        )
    };
    let (sort, desc) = (layout.sort, layout.descending);
    for top in TOPS {
        let Some(sections) = shown.remove(top) else {
            continue;
        };
        let everything: Vec<ProcessInfo> = all[top].values().flatten().cloned().collect();
        let (cpu, memory) = sums(&everything);
        let key = format!("cat:{top}");
        let top_collapsed = !expand_all && collapsed.contains(&key);
        rows.push(Row::Group {
            depth: 0,
            note: (top == MACOS).then_some("built into the system"),
            label: top.to_string(),
            count: everything.len(),
            cpu,
            memory,
            collapsed: top_collapsed,
            key,
        });
        if top_collapsed {
            continue;
        }
        let mut order: Vec<(String, f32, u64, usize)> = sections
            .keys()
            .map(|sec| {
                let v = &all[top][sec];
                let (c, m) = sums(v);
                (sec.clone(), c, m, v.len())
            })
            .collect();
        order.sort_by(|a, b| {
            let o = match sort {
                ColumnId::Memory | ColumnId::OwnMemory => a.2.cmp(&b.2),
                ColumnId::Cpu => a.1.total_cmp(&b.1),
                _ => b.0.to_lowercase().cmp(&a.0.to_lowercase()),
            };
            if desc { o.reverse() } else { o }
        });
        let mut sections = sections;
        for (sec, cpu, memory, count) in order {
            let key = format!("cat:{top}/{sec}");
            let sec_collapsed = !expand_all && collapsed.contains(&key);
            rows.push(Row::Group {
                depth: 1,
                note: None,
                label: sec.clone(),
                count,
                cpu,
                memory,
                collapsed: sec_collapsed,
                key,
            });
            if !sec_collapsed {
                let totals = subtree_totals(&all[top][&sec]);
                let items = sections.remove(&sec).unwrap_or_default();
                tree_rows(items, &totals, layout, 2, collapsed, expand_all, None, rows);
            }
        }
    }
}

/// Scroll only along the element's own scroll axis: without this, a scroller that only moves
/// vertically turns a sideways swipe into vertical movement (and vice versa).
fn axis_locked<E: gpui::Styled>(mut element: E) -> E {
    element.style().restrict_scroll_to_axis = Some(true);
    element
}

/// A keyboard move in the process list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    /// Collapse the branch, or (if already collapsed or a leaf) jump to the parent.
    Left,
    /// Expand the branch, or (if already expanded) step to its first child.
    Right,
    /// Fold or unfold the branch.
    Toggle,
    /// Unfold the branch and everything under it.
    ExpandAll,
}

/// A row the keyboard cursor can sit on: a process, or a group/section header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowId {
    Pid(u32),
    Group(String),
}

impl Row {
    pub fn id(&self) -> RowId {
        match self {
            Row::Process { info, .. } => RowId::Pid(info.pid),
            Row::Group { key, .. } => RowId::Group(key.clone()),
        }
    }

    pub fn depth(&self) -> usize {
        match self {
            Row::Process { depth, .. } | Row::Group { depth, .. } => *depth,
        }
    }

    /// (can fold, is folded)
    fn fold_state(&self) -> (bool, bool) {
        match self {
            Row::Process {
                descendants,
                collapsed,
                ..
            } => (*descendants > 0, *collapsed),
            Row::Group { collapsed, .. } => (true, *collapsed),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct NavResult {
    pub select: Option<RowId>,
    /// Row to fold or unfold.
    pub toggle: Option<RowId>,
    /// Unfold the whole subtree, not just one level.
    pub all: bool,
}

/// Work out the new cursor for a key press over the rows currently shown. Process rows and
/// group rows are treated alike: both can be selected, folded and unfolded.
pub fn navigate(rows: &[Row], cursor: Option<&RowId>, nav: Nav, page: usize) -> NavResult {
    if rows.is_empty() {
        return NavResult::default();
    }
    let last = rows.len() - 1;
    let at = |i: usize| NavResult {
        select: Some(rows[i.min(last)].id()),
        ..Default::default()
    };
    let Some(pos) = cursor.and_then(|c| rows.iter().position(|r| &r.id() == c)) else {
        // Nothing selected yet: Up/End start at the bottom, everything else at the top.
        return at(if matches!(nav, Nav::Up | Nav::End) {
            last
        } else {
            0
        });
    };
    let row = &rows[pos];
    let (foldable, folded) = row.fold_state();
    let here = || NavResult {
        select: Some(row.id()),
        toggle: Some(row.id()),
        all: false,
    };
    let page = page.max(1);
    match nav {
        Nav::Up => at(pos.saturating_sub(1)),
        Nav::Down => at(pos + 1),
        Nav::PageUp => at(pos.saturating_sub(page)),
        Nav::PageDown => at(pos + page),
        Nav::Home => at(0),
        Nav::End => at(last),
        Nav::Toggle if foldable => here(),
        Nav::ExpandAll if foldable => NavResult {
            all: true,
            ..here()
        },
        Nav::Right if foldable && folded => here(),
        Nav::Right if foldable => at(pos + 1),
        Nav::Left if foldable && !folded => here(),
        Nav::Left => {
            // Jump to the parent: the nearest row above with a smaller indent.
            let parent = rows[..pos].iter().rposition(|r| r.depth() < row.depth());
            at(parent.unwrap_or(pos))
        }
        _ => at(pos),
    }
}

/// Every process under `pid` that itself has children, from the full process list
/// (so branches already hidden under a folded parent are included).
pub fn branch_descendants(processes: &[ProcessInfo], pid: u32) -> Vec<u32> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in processes {
        if let Some(pp) = p.ppid.filter(|&pp| pp != p.pid) {
            children.entry(pp).or_default().push(p.pid);
        }
    }
    let mut out = Vec::new();
    let mut seen: HashSet<u32> = [pid].into();
    let mut stack = children.get(&pid).cloned().unwrap_or_default();
    while let Some(c) = stack.pop() {
        if !seen.insert(c) {
            continue;
        }
        if let Some(grand) = children.get(&c) {
            out.push(c);
            stack.extend(grand.iter().copied());
        }
    }
    out
}

impl OtmApp {
    /// Fold a branch together with every branch under it, so unfolding later opens one level at a
    /// time. Unfolding opens just this branch; with `all` (Option held) a folded branch opens
    /// completely, like Option-clicking a folder's triangle in Finder.
    pub fn toggle_branch(&mut self, pid: u32, all: bool) {
        let key = node_key(pid);
        if self.collapsed.contains(&key) {
            if all {
                return self.unfold_all(pid);
            }
            self.collapsed.remove(&key);
        } else {
            let below = branch_descendants(&self.snapshot.processes, pid);
            self.collapsed.insert(key);
            self.collapsed.extend(below.into_iter().map(node_key));
            self.collapsed.extend(self.launchd_groups(pid));
        }
        self.rebuild_rows();
    }

    /// The section and sub-group keys under launchd in the Tree view (empty for other processes).
    fn launchd_groups(&self, pid: u32) -> Vec<String> {
        if pid != 1 || self.layout.group_by != GroupBy::Parent {
            return Vec::new();
        }
        let mut keys = vec!["split:0".to_string(), "split:1".to_string()];
        for part in ["split:0", "split:1"] {
            keys.extend(
                self.group_contents(part)
                    .into_iter()
                    .filter(|k| k.starts_with("split:")),
            );
        }
        keys
    }

    /// Open a branch and every branch under it.
    pub fn unfold_all(&mut self, pid: u32) {
        self.collapsed.remove(&node_key(pid));
        for d in branch_descendants(&self.snapshot.processes, pid) {
            self.collapsed.remove(&node_key(d));
        }
        for k in self.launchd_groups(pid) {
            self.collapsed.remove(&k);
        }
        self.rebuild_rows();
    }

    /// Everything inside a group, worked out from the full process list (so it is known even while the
    /// group is folded): the keys of process branches (processes with children) and of nested groups.
    fn group_parts(&self, key: &str) -> (Vec<u32>, Vec<String>) {
        let procs = &self.snapshot.processes;
        let has_children: HashSet<u32> = procs
            .iter()
            .filter_map(|p| p.ppid.filter(|&pp| pp != p.pid))
            .collect();
        let by_pid: HashMap<u32, &ProcessInfo> = procs.iter().map(|p| (p.pid, p)).collect();
        let mut nested: Vec<String> = Vec::new();
        let members: Vec<u32> = if let Some(rest) = key.strip_prefix("split:") {
            // Tree view: a section under launchd ("split:0" OS, "split:1" yours) or a sub-group in it.
            let (part, sub) = match rest.split_once('/') {
                Some((p, s)) => (p.parse::<usize>().unwrap_or(1), Some(s)),
                None => (rest.parse::<usize>().unwrap_or(1), None),
            };
            let cats = crate::categories::categorize(procs);
            let place = |pid: u32| by_pid.get(&pid).map(|p| split_place(p, &cats));
            if sub.is_none() {
                let mut subs: Vec<String> = procs
                    .iter()
                    .filter(|p| p.ppid == Some(1) && p.pid != 1)
                    .filter_map(|p| place(p.pid))
                    .filter(|pl| pl.0 == part)
                    .map(|pl| format!("split:{part}/{}", pl.1))
                    .collect();
                subs.sort();
                subs.dedup();
                nested = subs;
            }
            procs
                .iter()
                .filter(|p| {
                    split_branch(p, &by_pid)
                        .and_then(place)
                        .is_some_and(|pl| pl.0 == part && sub.is_none_or(|s| pl.1 == s))
                })
                .map(|p| p.pid)
                .collect()
        } else if let Some(rest) = key.strip_prefix("cat:") {
            let cats = crate::categories::categorize(procs);
            let (top, sec) = match rest.split_once('/') {
                Some((t, s)) => (t, Some(s)),
                None => (rest, None),
            };
            if sec.is_none() {
                let mut secs: Vec<String> = cats
                    .values()
                    .filter(|c| c.0 == top)
                    .map(|c| format!("cat:{top}/{}", c.1))
                    .collect();
                secs.sort();
                secs.dedup();
                nested = secs;
            }
            procs
                .iter()
                .filter(|p| {
                    cats.get(&p.pid)
                        .is_some_and(|c| c.0 == top && sec.is_none_or(|s| c.1 == s))
                })
                .map(|p| p.pid)
                .collect()
        } else if self.layout.group_by == GroupBy::App {
            procs
                .iter()
                .filter(|p| p.owner_app == key)
                .map(|p| p.pid)
                .collect()
        } else {
            procs
                .iter()
                .filter(|p| p.user == key)
                .map(|p| p.pid)
                .collect()
        };
        nested.extend(
            members
                .iter()
                .filter(|pid| has_children.contains(pid))
                .map(|&pid| node_key(pid)),
        );
        (members, nested)
    }

    /// Fold keys of everything inside a group (process branches and nested groups).
    fn group_contents(&self, key: &str) -> Vec<String> {
        self.group_parts(key).1
    }

    /// Every process in a group, from the full process list.
    pub fn group_pids(&self, key: &str) -> Vec<u32> {
        self.group_parts(key).0
    }

    /// Fold or unfold a group/section header, with the same rules as a process branch: folding
    /// folds everything inside it, unfolding opens one level, and `all` (Option) opens everything.
    pub fn toggle_group(&mut self, key: &str, all: bool) {
        let inside = self.group_contents(key);
        if self.collapsed.contains(key) {
            self.collapsed.remove(key);
            if all {
                for k in &inside {
                    self.collapsed.remove(k);
                }
            }
        } else if all {
            // Option on an open group: open everything inside it.
            for k in &inside {
                self.collapsed.remove(k);
            }
        } else {
            self.collapsed.insert(key.to_string());
            self.collapsed.extend(inside);
        }
        self.rebuild_rows();
    }

    /// Put the cursor on a row (process or group) and select just that row. A group selects no
    /// process, so it clears the selection.
    pub fn set_cursor(&mut self, id: Option<RowId>) {
        self.selected = match &id {
            Some(RowId::Pid(pid)) => Some(*pid),
            _ => None,
        };
        self.selection = self.selected.into_iter().collect();
        self.anchor = id.clone();
        self.cursor = id;
    }

    /// Live refresh: processes that have exited leave the selection.
    pub fn drop_exited(&mut self) {
        let alive: HashSet<u32> = self.snapshot.processes.iter().map(|p| p.pid).collect();
        self.selection.retain(|pid| alive.contains(pid));
        if self.selected.is_some_and(|pid| !alive.contains(&pid)) {
            self.selected = None;
        }
    }

    /// The selected processes that are on screen, in list order. Actions apply to these, so a
    /// filter or a collapsed branch never acts on rows the user cannot see.
    pub fn selected_pids(&self) -> Vec<u32> {
        self.rows
            .iter()
            .filter_map(|r| match r {
                Row::Process { info, .. } if self.selection.contains(&info.pid) => Some(info.pid),
                _ => None,
            })
            .collect()
    }

    /// Tab-separated visible columns, one line per process, in the order given.
    pub fn process_lines(&self, pids: &[u32]) -> String {
        pids.iter()
            .filter_map(|pid| self.snapshot.processes.iter().find(|p| p.pid == *pid))
            .map(|p| {
                self.layout
                    .visible()
                    .map(|(_, c)| c.id.cell(p))
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A click on a row: plain selects it, ⌘ (or Ctrl) toggles it, Shift selects the range from
    /// the anchor. Group rows always just take the cursor.
    pub fn click_row(&mut self, id: RowId, m: gpui::Modifiers) {
        match id {
            RowId::Pid(_) if m.shift => self.extend_selection(id),
            RowId::Pid(_) if m.platform || m.control => self.toggle_selection(id),
            _ => self.set_cursor(Some(id)),
        }
    }

    fn toggle_selection(&mut self, id: RowId) {
        let RowId::Pid(pid) = id else {
            return;
        };
        if !self.selection.remove(&pid) {
            self.selection.insert(pid);
        }
        self.selected = Some(pid);
        self.anchor = Some(RowId::Pid(pid));
        self.cursor = Some(RowId::Pid(pid));
    }

    /// Select every process from the anchor to `to`, in the current row order.
    fn extend_selection(&mut self, to: RowId) {
        let from = self
            .anchor
            .clone()
            .or_else(|| self.cursor.clone())
            .unwrap_or_else(|| to.clone());
        let position = |id: &RowId| self.rows.iter().position(|r| &r.id() == id);
        let (Some(a), Some(b)) = (position(&from), position(&to)) else {
            return self.set_cursor(Some(to));
        };
        let (lo, hi) = (a.min(b), a.max(b));
        self.selection = self.rows[lo..=hi]
            .iter()
            .filter_map(|r| match r {
                Row::Process { info, .. } => Some(info.pid),
                Row::Group { .. } => None,
            })
            .collect();
        self.selected = match &to {
            RowId::Pid(pid) => Some(*pid),
            RowId::Group(_) => None,
        };
        self.cursor = Some(to);
    }

    /// ⌘A on the process table: every process row on screen.
    pub fn select_all_processes(&mut self) {
        self.selection = self
            .rows
            .iter()
            .filter_map(|r| match r {
                Row::Process { info, .. } => Some(info.pid),
                Row::Group { .. } => None,
            })
            .collect();
    }

    /// Esc with several processes selected: keep only the cursor row. Returns whether it did.
    pub fn collapse_selection(&mut self) -> bool {
        if self.is_listing() {
            return self.collapse_list_selection();
        }
        if self.page != Page::Processes || self.selection.len() < 2 {
            return false;
        }
        let cursor = self.cursor.clone();
        self.set_cursor(cursor);
        true
    }

    /// Apply a plain keyboard move: update the cursor, fold/unfold, and scroll the cursor into view.
    pub fn navigate_processes(&mut self, nav: Nav) {
        self.move_processes(nav, false);
    }

    /// Apply a keyboard move; `extend` (Shift) grows the selection from the anchor instead of
    /// selecting just the new row.
    pub fn move_processes(&mut self, nav: Nav, extend: bool) {
        let page = f32::from(self.scroll.0.borrow().base_handle.bounds().size.height) / ROW_HEIGHT;
        let cursor = self
            .cursor
            .clone()
            .or_else(|| self.selected.map(RowId::Pid));
        let result = navigate(
            &self.rows,
            cursor.as_ref(),
            nav,
            (page as usize).saturating_sub(1).max(1),
        );
        match (result.toggle, result.all) {
            (Some(RowId::Pid(pid)), true) => self.unfold_all(pid),
            (Some(RowId::Pid(pid)), false) => self.toggle_branch(pid, false),
            (Some(RowId::Group(key)), all) => self.toggle_group(&key, all),
            (None, _) => {}
        }
        match result.select {
            Some(id) if extend => self.extend_selection(id),
            select => self.set_cursor(select),
        }
        if let Some(id) = &self.cursor
            && let Some(ix) = self.rows.iter().position(|r| &r.id() == id)
        {
            // Scroll only as far as needed to show the row, so the list doesn't jump on every key.
            let (first, shown) = {
                let state = self.scroll.0.borrow();
                let handle = &state.base_handle;
                let top = (-f32::from(handle.offset().y) / ROW_HEIGHT).max(0.);
                let shown = (f32::from(handle.bounds().size.height) / ROW_HEIGHT).floor();
                (top.ceil() as usize, shown as usize)
            };
            if shown == 0 {
                self.scroll.scroll_to_item(ix, ScrollStrategy::Center);
            } else if ix < first {
                self.scroll.scroll_to_item(ix, ScrollStrategy::Top);
            } else if ix + 1 > first + shown.saturating_sub(1) {
                self.scroll.scroll_to_item(ix, ScrollStrategy::Bottom);
            }
        }
    }

    pub fn rebuild_rows(&mut self) {
        let filter = Filter::new(&self.chips, &self.query, &self.snapshot.processes);
        self.match_count = if filter.is_active() {
            self.snapshot
                .processes
                .iter()
                .filter(|p| filter.matches(p))
                .count()
        } else {
            self.snapshot.processes.len()
        };
        self.rows = Rc::new(build_rows_filtered(
            &self.snapshot.processes,
            &self.layout,
            &filter,
            &self.collapsed,
        ));
    }

    fn toolbar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.snapshot.processes.len();
        let targets = self.selected_pids();
        let subtitle = match &self.notice {
            Some((msg, err)) => div()
                .text_color(if *err { t.hot } else { t.good })
                .child(crate::filter_bar::notice_text(msg, *err)),
            None if self.filter_active() => div().text_color(t.text_dim).child(format!(
                "{} of {count} processes match · Esc clears",
                self.match_count
            )),
            None => div().text_color(t.text_dim).child(format!(
                "{count} running · ↑↓ select · ←→ fold · drag headers to reorder"
            )),
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .px_4()
            .pt_4()
            .pb_3()
            // Title and the actions for the selection.
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child("Processes"),
                            )
                            .child(subtitle.text_xs().truncate()),
                    )
                    // Always shown, and dimmed without a selection, so the toolbar never shifts.
                    .child(
                        ui::pill(t, "quit", "Quit", false)
                            .flex_shrink_0()
                            .when(targets.is_empty(), |d| d.opacity(0.4))
                            .when(!targets.is_empty(), |d| {
                                let pids = targets.clone();
                                d.on_click(cx.listener(move |this, _, _, cx| {
                                    this.run_context_signal(pids.clone(), libc::SIGTERM, "Quit", cx)
                                }))
                            })
                            .tooltip(ui::tip("Ask the selected processes to quit")),
                    )
                    .child(
                        ui::pill(t, "force-quit", "Force Quit…", false)
                            .debug_selector(|| "force-quit".into())
                            .flex_shrink_0()
                            .text_color(t.hot)
                            .when(targets.is_empty(), |d| d.opacity(0.4))
                            .when(!targets.is_empty(), |d| {
                                let pids = targets.clone();
                                d.on_click(cx.listener(move |this, _, _, cx| {
                                    this.ask_force_quit(pids.clone());
                                    cx.notify();
                                }))
                            })
                            .tooltip(ui::tip("Stop the selected processes at once (asks first)")),
                    )
                    .child(
                        ui::pill(t, "columns-btn", "Columns ▾", self.column_menu)
                            .flex_shrink_0()
                            .debug_selector(|| "columns-btn".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    // A press here while open already closed the menu as a click
                                    // outside it; don't reopen it straight away.
                                    if !std::mem::take(&mut this.column_menu_just_closed) {
                                        this.column_menu = !this.column_menu;
                                    }
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            // Filter and grouping; the grouping wraps under the filter in a narrow window.
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .child(self.filter_field(t, cx))
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(div().text_xs().text_color(t.text_dim).child("Group"))
                            .child(ui::segmented(t).children(GroupBy::ALL.map(|g| {
                                ui::pill(t, g.label(), g.label(), self.layout.group_by == g)
                                    .w(px(78.))
                                    .flex()
                                    .justify_center()
                                    .debug_selector(|| format!("group-{g:?}"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.layout.group_by = g;
                                        this.layout_changed(cx);
                                    }))
                            }))),
                    ),
            )
    }

    fn empty_state(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .top(px(170.))
            .left_0()
            .right_0()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("No processes match"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(t.text_dim)
                    .child("Try fewer filters, or a shorter search."),
            )
            .child(
                ui::pill(t, "empty-clear", "Clear filters", true)
                    .debug_selector(|| "empty-clear".into())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.clear_filter();
                        cx.notify();
                    })),
            )
    }

    fn column_menu(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("column-menu")
            .absolute()
            .top(px(64.))
            .right(px(16.))
            .w(px(260.))
            .p_2()
            .flex()
            .flex_col()
            .gap_0p5()
            .bg(t.glass_strong)
            .border_1()
            .border_color(t.highlight)
            .rounded(px(16.))
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.column_menu = false;
                this.column_menu_just_closed = true;
                cx.notify();
            }))
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.text_dim)
                    .child("SHOW, HIDE AND ORDER COLUMNS"),
            )
            .children(self.layout.columns.iter().enumerate().map(|(i, c)| {
                let id = c.id;
                let last = i + 1 == self.layout.columns.len();
                div()
                    .id(("col-opt", i))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded(px(8.))
                    .text_sm()
                    .hover(|h| h.bg(t.glass_hover))
                    .child(
                        div()
                            .id(("col-check", i))
                            .debug_selector(|| format!("col-check-{id:?}"))
                            .size(px(16.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .border_1()
                            .border_color(if c.visible { t.accent } else { t.stroke })
                            .when(c.visible, |d| d.bg(t.selection).text_color(gpui::white()))
                            .text_xs()
                            .cursor_pointer()
                            .child(if c.visible { "✓" } else { "" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.layout.toggle(id);
                                this.layout_changed(cx);
                            })),
                    )
                    // The name toggles the column too: a far bigger target than the box.
                    .child(
                        div()
                            .id(("col-name", i))
                            .flex_1()
                            .cursor_pointer()
                            .child(id.title())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.layout.toggle(id);
                                this.layout_changed(cx);
                            })),
                    )
                    .child(arrow(t, ("up", i), "↑", i > 0).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.layout.shift(id, -1);
                            this.layout_changed(cx);
                        },
                    )))
                    .child(arrow(t, ("down", i), "↓", !last).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.layout.shift(id, 1);
                            this.layout_changed(cx);
                        },
                    )))
            }))
            .child(
                div()
                    .id("reset-columns")
                    .mt_1()
                    .px_2()
                    .py_1()
                    .rounded(px(8.))
                    .text_xs()
                    .text_color(t.accent)
                    .cursor_pointer()
                    .hover(|h| h.bg(t.glass_hover))
                    .child("Reset to default layout")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let group = this.layout.group_by;
                        this.layout = Default::default();
                        this.layout.group_by = group;
                        this.layout_changed(cx);
                    })),
            )
    }

    fn header(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .h(px(30.))
            .items_center()
            .px_3()
            .border_b_1()
            .border_color(t.stroke)
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(t.text_dim)
            .children(self.layout.visible().map(|(index, c)| {
                let id = c.id;
                let width = c.width;
                let sorted = self.layout.sort == id;
                let arrow = if !sorted {
                    ""
                } else if self.layout.descending {
                    " ↓"
                } else {
                    " ↑"
                };
                div()
                    .id(("header", index))
                    .debug_selector(|| format!("header-{id:?}"))
                    .relative()
                    .flex_shrink_0()
                    .w(px(width))
                    .h_full()
                    .flex()
                    .items_center()
                    .when(id.numeric(), |d| d.justify_end())
                    .px_2()
                    .cursor_pointer()
                    .when(sorted, |d| d.text_color(t.accent))
                    .hover(|h| h.bg(t.glass_hover))
                    .rounded(px(6.))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(format!("{}{arrow}", id.title())),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.layout.sort_by(id);
                        this.layout_changed(cx);
                    }))
                    .on_drag(ColumnDrag { id }, |d, _, _, cx| cx.new(|_| d.clone()))
                    .drag_over::<ColumnDrag>(|s, _, _, _| s.border_l_2())
                    .on_drop(cx.listener(move |this, d: &ColumnDrag, _, cx| {
                        this.layout.move_column(d.id, id);
                        this.layout_changed(cx);
                    }))
                    .child(
                        div()
                            .id(("resize", index))
                            .debug_selector(|| format!("resize-{id:?}"))
                            .absolute()
                            .right(px(-3.))
                            .top_1()
                            .bottom_1()
                            .w(px(7.))
                            .cursor_col_resize()
                            .child(div().mx_auto().w(px(1.)).h_full().bg(t.stroke))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.resizing = Some(Resize {
                                        index,
                                        start_x: ev.position.x,
                                        start_width: width,
                                    });
                                }),
                            ),
                    )
            }))
    }

    fn render_row(&self, t: &Theme, ix: usize, row: &Row, cx: &mut Context<Self>) -> AnyElement {
        match row {
            Row::Group {
                depth,
                note,
                key,
                label,
                count,
                cpu,
                memory,
                collapsed,
            } => {
                // A group is a node in the same tree: same row, same indent and ▸/▾ in the Name
                // column, its totals in the CPU and Memory columns.
                let id = RowId::Group(key.clone());
                let selected = self.cursor.as_ref() == Some(&id);
                let tree_col = self.tree_column();
                let (depth, collapsed) = (*depth, *collapsed);
                let row_selector = format!("group-row-{key}");
                let twisty_selector = format!("group-twisty-{key}");
                let toggle_key = key.clone();
                let label = label.clone();
                let note = *note;
                let (count, cpu, memory) = (*count, *cpu, *memory);
                let menu_id = id.clone();
                let bundle = self.owner_bundles.get(&label).cloned();
                div()
                    .id(("group", ix))
                    .debug_selector(move || row_selector)
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                            this.set_cursor(Some(menu_id.clone()));
                            this.context_menu = Some((menu_id.clone(), ev.position));
                            window.focus(&this.focus);
                            cx.notify();
                        }),
                    )
                    .w_full()
                    .h(px(ROW_HEIGHT))
                    .mx_2()
                    .px_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .rounded(px(8.))
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .when(ix % 2 == 1 && !selected, |d| d.bg(t.row_alt))
                    .when(selected, |d| d.bg(t.selection).text_color(gpui::white()))
                    .when(!selected, |d| d.hover(|h| h.bg(t.glass_hover)))
                    .children(self.layout.visible().map(|(i, c)| {
                        let cell = div()
                            .flex_shrink_0()
                            .w(px(c.width))
                            .px_2()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .when(c.id.numeric(), |d| {
                                d.justify_end().font_family("Menlo").text_xs()
                            });
                        if i == tree_col {
                            let toggle_key = toggle_key.clone();
                            let twisty_selector = twisty_selector.clone();
                            // The label keeps its width; only the note after it truncates.
                            cell.overflow_hidden()
                                .child(div().flex_shrink_0().w(px(depth as f32 * 14.)))
                                .child(
                                    div()
                                        .id(("group-twisty", ix))
                                        .debug_selector(move || twisty_selector)
                                        .flex_shrink_0()
                                        .w(px(14.))
                                        .text_xs()
                                        .cursor_pointer()
                                        .text_color(if selected {
                                            gpui::white().into()
                                        } else {
                                            t.accent
                                        })
                                        .child(if collapsed { "▸" } else { "▾" })
                                        .on_click(cx.listener(
                                            move |this, ev: &gpui::ClickEvent, _, cx| {
                                                cx.stop_propagation();
                                                this.toggle_group(&toggle_key, ev.modifiers().alt);
                                                cx.notify();
                                            },
                                        )),
                                )
                                .child(self.icon_slot(bundle.as_deref()))
                                .child(div().flex_shrink_0().child(label.clone()))
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .px_1()
                                        .rounded_full()
                                        .bg(if selected {
                                            gpui::white().opacity(0.25)
                                        } else {
                                            t.accent_soft.into()
                                        })
                                        .text_xs()
                                        .font_weight(FontWeight::NORMAL)
                                        .text_color(if selected {
                                            gpui::white().into()
                                        } else {
                                            t.accent
                                        })
                                        .child(count.to_string()),
                                )
                                .when_some(note, |d, note| {
                                    d.child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .text_xs()
                                            .font_weight(FontWeight::NORMAL)
                                            .text_color(if selected {
                                                gpui::white().opacity(0.8)
                                            } else {
                                                t.text_dim.into()
                                            })
                                            .child(note),
                                    )
                                })
                        } else {
                            cell.child(match c.id {
                                ColumnId::Cpu => format!("{cpu:.1}%"),
                                ColumnId::Memory => format!("Σ {}", fmt_bytes(memory)),
                                _ => String::new(),
                            })
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_cursor(Some(id.clone()));
                        window.focus(&this.focus);
                        cx.notify();
                    }))
                    .into_any_element()
            }
            Row::Process {
                info,
                depth,
                descendants,
                collapsed,
                tree_memory,
            } => {
                let pid = info.pid;
                let selected = self.selection.contains(&pid);
                let tree = self.layout.group_by != GroupBy::None;
                let tree_col = self.tree_column();
                let (depth, descendants, collapsed, tree_memory) =
                    (*depth, *descendants, *collapsed, *tree_memory);
                let bundle = crate::icons::bundle_of(&info.path);
                let row_selector = format!("proc-row-{pid}");
                div()
                    .id(("proc", ix))
                    .debug_selector(move || row_selector)
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                            // Right-clicking inside the selection keeps it; anywhere else selects that row.
                            if !this.selected_pids().contains(&pid) {
                                this.set_cursor(Some(RowId::Pid(pid)));
                            }
                            this.context_menu = Some((RowId::Pid(pid), ev.position));
                            window.focus(&this.focus);
                            cx.notify();
                        }),
                    )
                    .h(px(ROW_HEIGHT))
                    .mx_2()
                    .px_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .rounded(px(8.))
                    .text_sm()
                    .cursor_default()
                    .when(ix % 2 == 1 && !selected, |d| d.bg(t.row_alt))
                    .when(selected, |d| d.bg(t.selection).text_color(gpui::white()))
                    .when(!selected, |d| d.hover(|h| h.bg(t.glass_hover)))
                    .children(self.layout.visible().map(|(i, c)| {
                        let text = if c.id == ColumnId::Memory {
                            memory_text(info.memory, tree_memory, descendants, tree)
                        } else {
                            c.id.cell(info)
                        };
                        let is_tree_col = tree && i == tree_col;
                        div()
                            .flex_shrink_0()
                            .w(px(c.width))
                            .overflow_hidden()
                            .px_2()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .when(c.id.numeric(), |d| {
                                d.justify_end().font_family("Menlo").text_xs()
                            })
                            .when(c.id == ColumnId::Cpu && !selected, |d| {
                                d.text_color(if info.cpu >= 50. {
                                    t.hot
                                } else if info.cpu >= 10. {
                                    t.warn
                                } else {
                                    t.text
                                })
                            })
                            .when(
                                matches!(c.id, ColumnId::Path | ColumnId::User | ColumnId::State)
                                    && !selected,
                                |d| d.text_color(t.text_dim),
                            )
                            .when(is_tree_col, |d| {
                                d.justify_start()
                                    .child(div().flex_shrink_0().w(px(depth as f32 * 14.)))
                                    .child(if descendants > 0 {
                                        div()
                                            .id(("twisty", ix))
                                            .debug_selector(|| format!("twisty-{pid}"))
                                            .flex_shrink_0()
                                            .w(px(14.))
                                            .text_xs()
                                            .cursor_pointer()
                                            .text_color(if selected {
                                                gpui::white().into()
                                            } else {
                                                t.accent
                                            })
                                            .child(if collapsed { "▸" } else { "▾" })
                                            .on_click(cx.listener(
                                                move |this, ev: &gpui::ClickEvent, _, cx| {
                                                    cx.stop_propagation();
                                                    this.toggle_branch(pid, ev.modifiers().alt);
                                                    cx.notify();
                                                },
                                            ))
                                            .into_any_element()
                                    } else {
                                        div().flex_shrink_0().w(px(14.)).into_any_element()
                                    })
                            })
                            .when(i == tree_col, |d| {
                                d.child(self.icon_slot(bundle.as_deref()))
                            })
                            // Text columns end in "…" in what is left of the cell after the indent,
                            // twisty, icon and badges (paths keep their end, "…" in the middle),
                            // and show the full text on hover. Numbers keep their right alignment.
                            .child(if c.id.numeric() {
                                div().flex_shrink_0().child(text).into_any_element()
                            } else {
                                let lead = if is_tree_col {
                                    depth as f32 * 14. + 14. + 8.
                                } else {
                                    0.
                                } + if i == tree_col { 18. } else { 0. };
                                let trail = if is_tree_col && collapsed {
                                    16. + 7. * (descendants.to_string().len() + 1) as f32
                                } else {
                                    0.
                                } + if c.id == ColumnId::Name
                                    && info.is_self
                                    && self.settings.mark_self
                                {
                                    64.
                                } else {
                                    0.
                                };
                                let room = c.width - 16. - lead - trail;
                                let shown = if c.id == ColumnId::Path {
                                    ui::ellipsize_middle(cx, text.clone(), room, self.text_px())
                                } else {
                                    ui::ellipsize(cx, text.clone(), room, self.text_px())
                                };
                                if shown == text {
                                    div().flex_shrink_0().child(shown).into_any_element()
                                } else {
                                    div()
                                        .id(("cell", ix * 64 + i))
                                        .flex_shrink_0()
                                        .child(shown)
                                        .tooltip(ui::tip(text))
                                        .into_any_element()
                                }
                            })
                            .when(is_tree_col && collapsed, |d| {
                                d.child(
                                    div()
                                        .flex_shrink_0()
                                        .px_1()
                                        .rounded_full()
                                        .bg(t.accent_soft)
                                        .text_xs()
                                        .text_color(t.accent)
                                        .child(format!("+{descendants}")),
                                )
                            })
                            .when(
                                c.id == ColumnId::Name && info.is_self && self.settings.mark_self,
                                |d| {
                                    d.child(
                                        div()
                                            .px_1()
                                            .rounded(px(4.))
                                            .border_1()
                                            .border_color(t.accent)
                                            .text_color(t.accent)
                                            .text_xs()
                                            .child("this app"),
                                    )
                                },
                            )
                    }))
                    .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, window, cx| {
                        this.click_row(RowId::Pid(pid), ev.modifiers());
                        window.focus(&this.focus);
                        cx.notify();
                    }))
                    .into_any_element()
            }
        }
    }

    /// Size of `text_sm` (0.875 rem) at the chosen text scale.
    pub(crate) fn text_px(&self) -> f32 {
        14. * self.settings.text_scale as f32 / 100.
    }

    /// The column that carries the tree indent: Name if shown, otherwise the first visible column.
    fn tree_column(&self) -> usize {
        let mut visible = self.layout.visible();
        self.layout
            .visible()
            .find(|(_, c)| c.id == ColumnId::Name)
            .or_else(|| visible.next())
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Width of all visible columns plus the row insets.
    pub fn table_width(&self) -> f32 {
        self.layout.visible().map(|(_, c)| c.width).sum::<f32>() + 32.
    }

    pub fn render_processes(&mut self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *t;
        let rows = self.rows.clone();
        ui::glass(t)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.toolbar(t, cx))
            .child(
                // Header and rows share one horizontally scrolling area as wide as all chosen columns,
                // so every column can be reached; the rows still scroll vertically on their own.
                axis_locked(
                    div()
                        .id("process-table")
                        .debug_selector(|| "process-table".into())
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .overflow_x_scroll()
                        .track_scroll(&self.hscroll),
                )
                .child(
                    div()
                        .h_full()
                        .w(px(self.table_width()))
                        .min_w_full()
                        .flex()
                        .flex_col()
                        .child(self.header(t, cx))
                        .child(axis_locked(
                            uniform_list(
                                "process-rows",
                                rows.len(),
                                cx.processor(move |this, range: Range<usize>, _, cx| {
                                    let rows = this.rows.clone();
                                    range
                                        .filter_map(|ix| {
                                            rows.get(ix)
                                                .map(|row| this.render_row(&theme, ix, row, cx))
                                        })
                                        .collect::<Vec<_>>()
                                }),
                            )
                            .flex_1()
                            .py_1()
                            .track_scroll(self.scroll.clone()),
                        )),
                ),
            )
            .when(
                self.rows.is_empty() && self.filter_active() && !self.typing_to_suggestion(),
                |d| d.child(self.empty_state(t, cx)),
            )
            .when(self.column_menu, |d| d.child(self.column_menu(t, cx)))
            .when_some(self.context_menu.clone(), |d, (id, at)| {
                d.child(deferred(
                    anchored()
                        .position(at)
                        .snap_to_window_with_margin(px(8.))
                        .child(self.render_context_menu(t, &id, cx)),
                ))
            })
    }

    /// A 14 px app icon (or an empty slot of the same width, so names stay aligned).
    pub(crate) fn icon_slot(&self, bundle: Option<&str>) -> AnyElement {
        match bundle.and_then(|b| self.icons.get(b)) {
            Some(path) => img(path.clone())
                .size(px(14.))
                .flex_shrink_0()
                .into_any_element(),
            None => div().w(px(14.)).flex_shrink_0().into_any_element(),
        }
    }
}

fn arrow(
    t: &Theme,
    id: (&'static str, usize),
    glyph: &'static str,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .size(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .text_xs()
        .when(enabled, |d| {
            d.text_color(t.text_dim)
                .cursor_pointer()
                .hover(|h| h.bg(t.glass_strong))
        })
        .when(!enabled, |d| d.text_color(t.text_faint))
        .child(glyph)
}
