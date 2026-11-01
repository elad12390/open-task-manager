//! Auto-fit for a table column (double-clicking its header divider): the width that shows the
//! widest header and cell across all rows, measured with GPUI's text system and laid out the way
//! the rows lay out their cells.

use std::collections::HashMap;

use gpui::{Context, FontWeight, TextRun, Window, black, font, px};

use crate::{
    app::OtmApp,
    columns::{ColumnId, GroupBy},
    metrics::{fmt_bytes, fmt_duration, fmt_size},
    pages::LIST_ICON_LEAD,
    processes::{Row, memory_text},
    sysops,
};

/// `px_2` on each side of a cell.
const CELL_PAD: f32 = 16.;
/// `gap_1` between the children of a cell.
const GAP: f32 = 4.;
/// Indent per tree level, and the width of the twisty and of the app icon.
const INDENT: f32 = 14.;
const SLOT: f32 = 14.;
/// `px_1` on each side of a badge.
const BADGE_PAD: f32 = 8.;
const MIN_WIDTH: f32 = 40.;
const MAX_WIDTH: f32 = 900.;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Face {
    Text,
    Strong,
    Mono,
}

struct Measure<'a> {
    window: &'a Window,
    cache: HashMap<(Face, u32, String), f32>,
}

impl Measure<'_> {
    fn width(&mut self, text: &str, face: Face, size: f32) -> f32 {
        if text.is_empty() {
            return 0.;
        }
        let key = (face, size.to_bits(), text.to_string());
        if let Some(w) = self.cache.get(&key) {
            return *w;
        }
        let mut f = font(if face == Face::Mono {
            "Menlo"
        } else {
            ".SystemUIFont"
        });
        f.weight = if face == Face::Strong {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        };
        let run = TextRun {
            len: text.len(),
            font: f,
            color: black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let w = f32::from(
            self.window
                .text_system()
                .shape_line(text.to_string().into(), px(size), &[run], None)
                .width,
        );
        self.cache.insert(key, w);
        w
    }
}

/// A cell's width: its padding plus its children and the gaps between them.
fn cell(children: &[f32]) -> f32 {
    CELL_PAD + children.iter().sum::<f32>() + GAP * children.len().saturating_sub(1) as f32
}

/// The width column `index` needs to show every header and cell in full.
pub fn fitted_width(app: &OtmApp, index: usize, window: &Window) -> Option<f32> {
    let id = app.layout.columns.get(index)?.id;
    let tree = app.layout.group_by != GroupBy::None;
    let name_col = index == app.tree_column();
    let text_px = app.text_px();
    // text_xs is 12/14 of text_sm, the size the rows use for text.
    let small = text_px * 12. / 14.;
    let mut m = Measure {
        window,
        cache: HashMap::new(),
    };
    let mut widest = cell(&[m.width(id.title(), Face::Strong, small), SLOT]);
    for row in app.rows.iter() {
        let needed = match row {
            Row::Group {
                depth,
                note,
                label,
                count,
                cpu,
                memory,
                ..
            } => {
                if name_col {
                    let mut kids = vec![
                        *depth as f32 * INDENT,
                        SLOT,
                        SLOT,
                        m.width(label, Face::Strong, text_px),
                        m.width(&count.to_string(), Face::Text, small) + BADGE_PAD,
                    ];
                    if let Some(note) = note {
                        kids.push(m.width(note, Face::Text, small));
                    }
                    cell(&kids)
                } else {
                    let text = match id {
                        ColumnId::Cpu => format!("{cpu:.1}%"),
                        ColumnId::Memory => format!("Σ {}", fmt_bytes(*memory)),
                        _ => String::new(),
                    };
                    cell(&[m.width(&text, Face::Mono, small)])
                }
            }
            Row::Process {
                info,
                depth,
                descendants,
                collapsed,
                tree_memory,
            } => {
                let text = if id == ColumnId::Memory {
                    memory_text(info.memory, *tree_memory, *descendants, tree)
                } else {
                    id.cell(info)
                };
                if id.numeric() {
                    cell(&[m.width(&text, Face::Mono, small)])
                } else {
                    let mut kids = Vec::new();
                    if name_col && tree {
                        kids.push(*depth as f32 * INDENT);
                        kids.push(SLOT);
                    }
                    if name_col {
                        kids.push(SLOT);
                    }
                    kids.push(m.width(&text, Face::Text, text_px));
                    if name_col && tree && *collapsed {
                        kids.push(
                            m.width(&format!("+{descendants}"), Face::Text, small) + BADGE_PAD,
                        );
                    }
                    if id == ColumnId::Name && info.is_self && app.settings.mark_self {
                        kids.push(m.width("this app", Face::Text, small) + BADGE_PAD + 2.);
                    }
                    cell(&kids)
                }
            }
        };
        widest = widest.max(needed);
    }
    Some(widest.clamp(MIN_WIDTH, MAX_WIDTH))
}

/// The width a column needs for its header and every text in it (`mono` for the figures of a
/// listing's numeric columns); `lead` is room for an icon before the first text.
fn fitted_texts(
    window: &Window,
    title: &str,
    text_px: f32,
    mono: bool,
    lead: f32,
    texts: impl Iterator<Item = String>,
) -> f32 {
    let small = text_px * 12. / 14.;
    let mut m = Measure {
        window,
        cache: HashMap::new(),
    };
    let (face, size) = if mono {
        (Face::Mono, small)
    } else {
        (Face::Text, text_px)
    };
    let mut widest = cell(&[m.width(title, Face::Strong, small), SLOT]);
    for text in texts {
        let mut kids = Vec::new();
        if lead > 0. {
            kids.push(lead);
        }
        kids.push(m.width(&text, face, size));
        widest = widest.max(cell(&kids));
    }
    widest.clamp(MIN_WIDTH, MAX_WIDTH)
}

impl OtmApp {
    /// Double-clicking a column divider: the column takes the width its content needs.
    pub fn fit_column(&mut self, index: usize, window: &Window, cx: &mut Context<Self>) {
        self.resizing = None;
        if let Some(width) = fitted_width(self, index, window) {
            self.layout.columns[index].width = width;
            self.layout_changed(cx);
        }
    }

    /// The cell texts of a table column over all its rows, and the room an icon takes before them.
    fn column_texts(&self, table: &str, index: usize) -> (Vec<String>, f32) {
        match table {
            "folders" => {
                let items = self
                    .scans
                    .first()
                    .map(|s| s.items.as_slice())
                    .unwrap_or(&[]);
                let texts = items
                    .iter()
                    .map(|f| match index {
                        0 => f.path.clone(),
                        1 => fmt_size(f.bytes),
                        _ => String::new(),
                    })
                    .collect();
                (texts, if index == 0 { LIST_ICON_LEAD } else { 0. })
            }
            "scans" => {
                let now = sysops::now_secs();
                let texts = self
                    .scans
                    .iter()
                    .map(|s| match index {
                        0 => format!("{} ago", fmt_duration(now.saturating_sub(s.at))),
                        1 => s.items.len().to_string(),
                        _ => fmt_size(s.items.iter().map(|i| i.bytes).sum()),
                    })
                    .collect();
                (texts, 0.)
            }
            _ => {
                let texts = self
                    .listing
                    .rows
                    .iter()
                    .map(|r| {
                        r.cells
                            .get(index)
                            .map(|c| c.to_string())
                            .unwrap_or_default()
                    })
                    .collect();
                let lead = if index == 0 && self.listing.icons {
                    LIST_ICON_LEAD
                } else {
                    0.
                };
                (texts, lead)
            }
        }
    }

    /// Double-clicking a column edge of a listing or Disk table: the column takes the width its
    /// header and every cell need.
    pub fn fit_list_column(
        &mut self,
        table: &'static str,
        pos: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.list_resizing = None;
        let Some(col) = self
            .table_layouts
            .get(table)
            .and_then(|l| l.columns.get(pos))
            .cloned()
        else {
            return;
        };
        let (texts, lead) = self.column_texts(table, col.index);
        let mono = col.numeric && !matches!(table, "folders" | "scans");
        let width = fitted_texts(
            window,
            col.title,
            self.text_px(),
            mono,
            lead,
            texts.into_iter(),
        );
        if let Some(l) = self.table_layouts.get_mut(table) {
            l.set_width(pos, width);
        }
        self.save_table_layouts();
        cx.notify();
    }
}
