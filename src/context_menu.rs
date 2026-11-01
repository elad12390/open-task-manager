//! Right-click menu on process and group rows: quit / force quit / suspend, reveal in Finder,
//! show connections, copy, fold.

use std::collections::HashMap;

use gpui::{ClipboardItem, Context, FontWeight, div, prelude::*, px};

use crate::{
    app::{OtmApp, Page},
    categories::OS_OWNER,
    filter::Chip,
    icons::bundle_of,
    metrics::fmt_bytes,
    processes::{RowId, node_key},
    sysops,
    theme::Theme,
};

#[derive(Clone, Debug)]
pub enum Cmd {
    Signal {
        pids: Vec<u32>,
        sig: i32,
        verb: &'static str,
    },
    Reveal(String),
    Connections(String),
    FilterApp(String),
    /// What is copied (for the notice), and the text.
    Copy(&'static str, String),
    Fold(RowId),
}

/// A pending Force Quit, waiting for the user to confirm it.
#[derive(Clone, Debug)]
pub struct Confirm {
    pub pids: Vec<u32>,
    pub title: String,
    pub detail: String,
}

pub struct Item {
    pub label: String,
    /// None draws a separator.
    pub cmd: Option<Cmd>,
    pub danger: bool,
    pub enabled: bool,
}

fn item(label: impl Into<String>, cmd: Cmd) -> Item {
    Item {
        label: label.into(),
        cmd: Some(cmd),
        danger: false,
        enabled: true,
    }
}

fn separator() -> Item {
    Item {
        label: String::new(),
        cmd: None,
        danger: false,
        enabled: false,
    }
}

impl OtmApp {
    fn descendants_of(&self, pid: u32) -> Vec<u32> {
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for p in &self.snapshot.processes {
            if let Some(pp) = p.ppid.filter(|&pp| pp != p.pid) {
                children.entry(pp).or_default().push(p.pid);
            }
        }
        let mut out = Vec::new();
        let mut stack = children.get(&pid).cloned().unwrap_or_default();
        while let Some(c) = stack.pop() {
            if !out.contains(&c) && c != pid {
                out.push(c);
                stack.extend(children.get(&c).cloned().unwrap_or_default());
            }
        }
        out
    }

    /// The menu for a row, without items that are unavailable here (macOS hides those in context
    /// menus) and without separators left leading, trailing or doubled.
    pub fn context_items(&self, id: &RowId) -> Vec<Item> {
        let mut out: Vec<Item> = Vec::new();
        for it in self
            .all_context_items(id)
            .into_iter()
            .filter(|i| i.cmd.is_none() || i.enabled)
        {
            let sep = it.cmd.is_none();
            if sep && out.last().is_none_or(|l| l.cmd.is_none()) {
                continue;
            }
            out.push(it);
        }
        if out.last().is_some_and(|l| l.cmd.is_none()) {
            out.pop();
        }
        out
    }

    /// The menu for a right-click inside a multi-selection: the same actions for the whole selection.
    fn selection_items(&self) -> Vec<Item> {
        let (pids, skipped) = self.selected_pids_and_skipped();
        let n = pids.len();
        let rows = self.selected_rows().len();
        let pid_list = pids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let note = match skipped {
            0 => String::new(),
            k => format!(" · {k} macOS skipped"),
        };
        vec![
            Item {
                enabled: n > 0,
                ..item(
                    format!(
                        "Quit {n} {}{note}",
                        crate::ui::plural(n, "Process", "Processes")
                    ),
                    Cmd::Signal {
                        pids: pids.clone(),
                        sig: libc::SIGTERM,
                        verb: "Quit",
                    },
                )
            },
            Item {
                danger: true,
                enabled: n > 0,
                ..item(
                    format!(
                        "Force Quit {n} {}{note}…",
                        crate::ui::plural(n, "Process", "Processes")
                    ),
                    Cmd::Signal {
                        pids,
                        sig: libc::SIGKILL,
                        verb: "Force quit",
                    },
                )
            },
            separator(),
            item(
                format!("Copy {rows} {}", crate::ui::plural(rows, "Row", "Rows")),
                Cmd::Copy("rows", self.selection_lines()),
            ),
            item("Copy PIDs", Cmd::Copy("PIDs", pid_list)),
        ]
    }

    fn all_context_items(&self, id: &RowId) -> Vec<Item> {
        if self.selection.len() > 1 && self.selection.contains(id) {
            return self.selection_items();
        }
        match id {
            RowId::Pid(pid) => {
                let Some(p) = self.snapshot.processes.iter().find(|p| p.pid == *pid) else {
                    return vec![item("Copy PID", Cmd::Copy("PID", pid.to_string()))];
                };
                let below = self.descendants_of(p.pid);
                let mut items = vec![item(
                    "Quit",
                    Cmd::Signal {
                        pids: vec![p.pid],
                        sig: libc::SIGTERM,
                        verb: "Quit",
                    },
                )];
                // Ask ps for the live state: the sampled one can be a second old.
                let stopped = sysops::process_state(p.pid).is_some_and(|s| s.contains('T'));
                items.push(if stopped {
                    item(
                        "Resume",
                        Cmd::Signal {
                            pids: vec![p.pid],
                            sig: libc::SIGCONT,
                            verb: "Resumed",
                        },
                    )
                } else {
                    item(
                        "Suspend",
                        Cmd::Signal {
                            pids: vec![p.pid],
                            sig: libc::SIGSTOP,
                            verb: "Suspended",
                        },
                    )
                });
                // Force quitting loses unsaved work: kept apart from Quit, and confirmed first.
                items.push(separator());
                items.push(Item {
                    danger: true,
                    ..item(
                        "Force Quit…",
                        Cmd::Signal {
                            pids: vec![p.pid],
                            sig: libc::SIGKILL,
                            verb: "Force quit",
                        },
                    )
                });
                if !below.is_empty() {
                    let mut all = below.clone();
                    all.push(p.pid);
                    items.push(Item {
                        danger: true,
                        ..item(
                            format!(
                                "Force Quit with {} {}…",
                                below.len(),
                                crate::ui::plural(below.len(), "Child", "Children")
                            ),
                            Cmd::Signal {
                                pids: all,
                                sig: libc::SIGKILL,
                                verb: "Force quit",
                            },
                        )
                    });
                }
                items.push(separator());
                items.push(Item {
                    enabled: !p.path.is_empty(),
                    ..item("Show in Finder", Cmd::Reveal(p.path.clone()))
                });
                if let Some(bundle) = bundle_of(&p.path) {
                    items.push(item("Show App in Finder", Cmd::Reveal(bundle)));
                }
                items.push(item("Show Connections", Cmd::Connections(p.name.clone())));
                if !p.owner_app.is_empty() && !self.chips.contains(&Chip::App(p.owner_app.clone()))
                {
                    items.push(item(
                        format!("Filter to {}", p.owner_app),
                        Cmd::FilterApp(p.owner_app.clone()),
                    ));
                }
                items.push(separator());
                items.push(item("Copy PID", Cmd::Copy("PID", p.pid.to_string())));
                items.push(Item {
                    enabled: !p.path.is_empty(),
                    ..item("Copy Path", Cmd::Copy("path", p.path.clone()))
                });
                items.push(item(
                    "Copy Info",
                    Cmd::Copy(
                        "info",
                        format!(
                            "{} (PID {})\nOwner: {}\nUser: {}\nCPU: {:.1}%\nMemory: {}\nPath: {}",
                            p.name,
                            p.pid,
                            p.owner_app,
                            p.user,
                            p.cpu,
                            fmt_bytes(p.memory),
                            p.path
                        ),
                    ),
                ));
                if !below.is_empty() {
                    items.push(separator());
                    let folded = self.folds().contains(&node_key(p.pid));
                    items.push(item(
                        if folded { "Unfold" } else { "Fold" },
                        Cmd::Fold(id.clone()),
                    ));
                }
                items
            }
            RowId::Group(key) => {
                let label = self
                    .rows
                    .iter()
                    .find_map(|r| match r {
                        crate::processes::Row::Group { key: k, label, .. } if k == key => {
                            Some(label.clone())
                        }
                        _ => None,
                    })
                    .unwrap_or_default();
                let pids = self.group_pids(key);
                // Mass-killing macOS's own processes would take the system down with them.
                let system =
                    label == OS_OWNER || key.starts_with("split:0") || key.starts_with("cat:macOS");
                let n = pids.len();
                let mut items = vec![
                    Item {
                        enabled: !system && n > 0,
                        ..item(
                            format!("Quit All {n}"),
                            Cmd::Signal {
                                pids: pids.clone(),
                                sig: libc::SIGTERM,
                                verb: "Quit",
                            },
                        )
                    },
                    Item {
                        danger: true,
                        enabled: !system && n > 0,
                        ..item(
                            format!("Force Quit All {n}…"),
                            Cmd::Signal {
                                pids,
                                sig: libc::SIGKILL,
                                verb: "Force quit",
                            },
                        )
                    },
                ];
                if let Some(bundle) = self.owner_bundles.get(&label) {
                    items.push(separator());
                    items.push(item("Show App in Finder", Cmd::Reveal(bundle.clone())));
                }
                items.push(separator());
                items.push(item("Copy Name", Cmd::Copy("name", label)));
                let folded = self.folds().contains(key);
                items.push(item(
                    if folded { "Unfold" } else { "Fold" },
                    Cmd::Fold(id.clone()),
                ));
                items
            }
        }
    }

    /// Ask before force quitting: name the process, or the first few of a batch.
    pub fn ask_force_quit(&mut self, pids: Vec<u32>) {
        let names: Vec<String> = pids
            .iter()
            .filter_map(|pid| self.snapshot.processes.iter().find(|p| p.pid == *pid))
            .map(|p| p.name.clone())
            .collect();
        let title = match names.as_slice() {
            [one] => format!("Force quit “{one}”?"),
            _ => format!(
                "Force quit {} {}?",
                pids.len(),
                crate::ui::plural(pids.len(), "process", "processes")
            ),
        };
        let detail = "Unsaved changes will be lost.".to_string();
        self.confirm = Some(Confirm {
            pids,
            title,
            detail,
        });
    }

    pub fn run_context(&mut self, cmd: Cmd, cx: &mut Context<Self>) {
        self.context_menu = None;
        match cmd {
            Cmd::Signal { pids, sig, .. } if sig == libc::SIGKILL => self.ask_force_quit(pids),
            Cmd::Signal { pids, sig, verb } => self.run_context_signal(pids, sig, verb, cx),
            Cmd::Reveal(path) => sysops::reveal_in_finder(&path),
            Cmd::Connections(name) => {
                self.open_page(Page::Connections, cx);
                self.query = name;
            }
            Cmd::FilterApp(app) => self.add_chip(Chip::App(app)),
            Cmd::Copy(what, text) => {
                self.notice = Some((format!("Copied {what}"), false));
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            Cmd::Fold(RowId::Pid(pid)) => self.toggle_branch(pid, false),
            Cmd::Fold(RowId::Group(key)) => self.toggle_group(&key, false),
        }
        cx.notify();
    }

    pub fn run_context_signal(
        &mut self,
        pids: Vec<u32>,
        sig: i32,
        verb: &'static str,
        cx: &mut Context<Self>,
    ) {
        {
            {
                if self.demo_mode {
                    // Never signal a real process from the demo.
                    let n = pids.len();
                    self.demo_kill(&pids);
                    self.notice = Some((
                        format!(
                            "{verb} {n} {}",
                            crate::ui::plural(n, "process", "processes")
                        ),
                        false,
                    ));
                    cx.notify();
                    return;
                }
                let n = pids.len();
                self.run_bg(
                    cx,
                    move || {
                        pids.iter()
                            .map(|&pid| sysops::send_signal(pid, sig))
                            .filter_map(Result::err)
                            .collect::<Vec<_>>()
                    },
                    move |app, errors, _| {
                        app.notice = Some(if errors.is_empty() {
                            (
                                format!(
                                    "{verb} {n} {}",
                                    crate::ui::plural(n, "process", "processes")
                                ),
                                false,
                            )
                        } else {
                            (
                                format!("Couldn't finish {} of {n}: {}", errors.len(), errors[0]),
                                true,
                            )
                        });
                        app.update_now = true;
                    },
                );
            }
        }
        cx.notify();
    }

    pub fn render_context_menu(
        &self,
        t: &Theme,
        id: &RowId,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("context-menu")
            .debug_selector(|| "context-menu".into())
            .occlude()
            .w(px(250.))
            .p_1()
            .flex()
            .flex_col()
            // Near-opaque like a macOS menu, so the rows behind never compete with its text.
            .bg(crate::theme::with_alpha(t.glass_strong, 0.97))
            .border_1()
            .border_color(t.highlight)
            .rounded(px(10.))
            .shadow_lg()
            .text_sm()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.context_menu = None;
                cx.notify();
            }))
            .children(
                self.context_items(id)
                    .into_iter()
                    .enumerate()
                    .map(|(i, it)| {
                        let Some(cmd) = it.cmd else {
                            return div()
                                .my_1()
                                .mx_2()
                                .h(px(1.))
                                .bg(t.stroke)
                                .into_any_element();
                        };
                        let selector = format!("ctx-{}", it.label);
                        div()
                            .id(("ctx", i))
                            .debug_selector(move || selector)
                            .px_2()
                            .py(px(3.))
                            .rounded(px(6.))
                            .font_weight(FontWeight::NORMAL)
                            .when(!it.enabled, |d| d.text_color(t.text_faint))
                            .when(it.enabled && it.danger, |d| d.text_color(t.hot))
                            .when(it.enabled, |d| {
                                d.cursor_pointer()
                                    .hover(|h| h.bg(t.selection).text_color(gpui::white()))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.run_context(cmd.clone(), cx)
                                    }))
                            })
                            .child(it.label)
                            .into_any_element()
                    }),
            )
    }

    /// The Force Quit confirmation: a sheet over a dimmed window. Cancel is the default (Return and
    /// Esc pick it); the red Force Quit button has to be clicked.
    pub fn render_confirm(
        &self,
        t: &Theme,
        c: Confirm,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let pids = c.pids.clone();
        div()
            .id("confirm-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000052))
            .occlude()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("confirm")
                    .debug_selector(|| "confirm".into())
                    .w(px(380.))
                    .p_5()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .rounded(px(16.))
                    .bg(crate::theme::with_alpha(t.glass_strong, 0.98))
                    .border_1()
                    .border_color(t.highlight)
                    .shadow_lg()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::BOLD)
                            .child(c.title.clone()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(t.text_dim)
                            .child(c.detail.clone()),
                    )
                    .child(
                        div()
                            .mt_3()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap_2()
                            .child(
                                div()
                                    .id("confirm-cancel")
                                    .debug_selector(|| "confirm-cancel".into())
                                    .px_4()
                                    .py(px(6.))
                                    .rounded(px(8.))
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .bg(t.selection)
                                    .text_color(gpui::white())
                                    .child("Cancel")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.confirm = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("confirm-force-quit")
                                    .debug_selector(|| "confirm-force-quit".into())
                                    .px_4()
                                    .py(px(6.))
                                    .rounded(px(8.))
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .bg(gpui::rgb(0xc4001a))
                                    .text_color(gpui::white())
                                    .child("Force Quit")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.confirm = None;
                                        this.run_context_signal(
                                            pids.clone(),
                                            libc::SIGKILL,
                                            "Force quit",
                                            cx,
                                        );
                                    })),
                            ),
                    ),
            )
    }
}
