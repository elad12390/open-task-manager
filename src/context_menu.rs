//! Right-click menu on process and group rows: quit / force quit / suspend, reveal in Finder,
//! show connections, copy, fold.

use std::collections::HashMap;

use gpui::{ClipboardItem, Context, FontWeight, div, prelude::*, px};

use crate::{
    app::{OtmApp, Page},
    categories::OS_OWNER,
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
    Copy(String),
    Fold(RowId),
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

    /// The menu for a row.
    pub fn context_items(&self, id: &RowId) -> Vec<Item> {
        match id {
            RowId::Pid(pid) => {
                let Some(p) = self.snapshot.processes.iter().find(|p| p.pid == *pid) else {
                    return vec![item("Copy PID", Cmd::Copy(pid.to_string()))];
                };
                let below = self.descendants_of(p.pid);
                let mut items = vec![
                    item(
                        "Quit",
                        Cmd::Signal {
                            pids: vec![p.pid],
                            sig: libc::SIGTERM,
                            verb: "Quit",
                        },
                    ),
                    Item {
                        danger: true,
                        ..item(
                            "Force Quit",
                            Cmd::Signal {
                                pids: vec![p.pid],
                                sig: libc::SIGKILL,
                                verb: "Force quit",
                            },
                        )
                    },
                ];
                if !below.is_empty() {
                    let mut all = below.clone();
                    all.push(p.pid);
                    items.push(Item {
                        danger: true,
                        ..item(
                            format!("Force Quit with {} Children", below.len()),
                            Cmd::Signal {
                                pids: all,
                                sig: libc::SIGKILL,
                                verb: "Force quit",
                            },
                        )
                    });
                }
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
                items.push(separator());
                items.push(Item {
                    enabled: !p.path.is_empty(),
                    ..item("Show in Finder", Cmd::Reveal(p.path.clone()))
                });
                if let Some(bundle) = bundle_of(&p.path) {
                    items.push(item("Show App in Finder", Cmd::Reveal(bundle)));
                }
                items.push(item("Show Connections", Cmd::Connections(p.name.clone())));
                items.push(separator());
                items.push(item("Copy PID", Cmd::Copy(p.pid.to_string())));
                items.push(Item {
                    enabled: !p.path.is_empty(),
                    ..item("Copy Path", Cmd::Copy(p.path.clone()))
                });
                items.push(item(
                    "Copy Info",
                    Cmd::Copy(format!(
                        "{} (PID {})\nOwner: {}\nUser: {}\nCPU: {:.1}%\nMemory: {}\nPath: {}",
                        p.name,
                        p.pid,
                        p.owner_app,
                        p.user,
                        p.cpu,
                        fmt_bytes(p.memory),
                        p.path
                    )),
                ));
                if !below.is_empty() {
                    items.push(separator());
                    let folded = self.collapsed.contains(&node_key(p.pid));
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
                            format!("Force Quit All {n}"),
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
                items.push(item("Copy Name", Cmd::Copy(label)));
                let folded = self.collapsed.contains(key);
                items.push(item(
                    if folded { "Unfold" } else { "Fold" },
                    Cmd::Fold(id.clone()),
                ));
                items
            }
        }
    }

    pub fn run_context(&mut self, cmd: Cmd, cx: &mut Context<Self>) {
        self.context_menu = None;
        match cmd {
            Cmd::Signal { pids, sig, verb } => {
                if self.demo_mode {
                    // Never signal a real process from the demo.
                    let n = pids.len();
                    self.demo_kill(&pids);
                    self.notice = Some((
                        format!("{verb} {n} process{}", if n == 1 { "" } else { "es" }),
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
                                format!("{verb} {n} process{}", if n == 1 { "" } else { "es" }),
                                false,
                            )
                        } else {
                            (
                                format!("{verb}: {} of {n} failed ({})", errors.len(), errors[0]),
                                true,
                            )
                        });
                        app.update_now = true;
                    },
                );
            }
            Cmd::Reveal(path) => sysops::reveal_in_finder(&path),
            Cmd::Connections(name) => {
                self.open_page(Page::Connections, cx);
                self.query = name;
            }
            Cmd::Copy(text) => {
                self.notice = Some((
                    format!("Copied {}", text.lines().next().unwrap_or_default()),
                    false,
                ));
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            Cmd::Fold(RowId::Pid(pid)) => self.toggle_branch(pid, false),
            Cmd::Fold(RowId::Group(key)) => self.toggle_group(&key, false),
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
                                    .hover(|h| h.bg(t.accent).text_color(gpui::white()))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.run_context(cmd.clone(), cx)
                                    }))
                            })
                            .child(it.label)
                            .into_any_element()
                    }),
            )
    }
}
