//! Auto-fit for a table column (double-clicking its header divider): the width that shows the
//! widest header and cell across all rows, measured with GPUI's text system and laid out the way
//! the rows lay out their cells.

use std::collections::HashMap;

use gpui::{Context, FontWeight, TextRun, Window, black, font, px};

use crate::{
    app::OtmApp,
    columns::{ColumnId, GroupBy},
    metrics::fmt_bytes,
    processes::{Row, memory_text},
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
    let mut widest = cell(&[m.width(&format!("{} ↓", id.title()), Face::Strong, small)]);
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

impl OtmApp {
    /// Double-clicking a column divider: the column takes the width its content needs.
    pub fn fit_column(&mut self, index: usize, window: &Window, cx: &mut Context<Self>) {
        self.resizing = None;
        if let Some(width) = fitted_width(self, index, window) {
            self.layout.columns[index].width = width;
            self.layout_changed(cx);
        }
    }
}
