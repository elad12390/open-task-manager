//! Tables outside the process list: sort keys, each table's column layout (order, widths, shown
//! columns and sort, saved across launches), and the header and Columns menu they share. A header
//! sorts, reorders by drag, resizes from its right edge and auto-fits on a double-click.

use std::{cmp::Ordering, collections::HashMap, fs, path::PathBuf};

use gpui::{
    Context, Div, FontWeight, MouseButton, MouseDownEvent, Pixels, Render, ScrollHandle,
    SharedString, Stateful, Window, div, prelude::*, px,
};
use serde::{Deserialize, Serialize};

use crate::{
    app::OtmApp,
    processes::axis_locked,
    scrollbar::{self, Axis},
    theme::Theme,
    ui,
};

#[derive(Clone, Debug, PartialEq)]
pub enum SortKey {
    Num(f64),
    Text(String),
}

impl SortKey {
    pub fn text(s: impl AsRef<str>) -> Self {
        Self::Text(s.as_ref().to_lowercase())
    }

    pub fn num(x: impl Into<f64>) -> Self {
        Self::Num(x.into())
    }

    pub fn compare(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Num(a), Self::Num(b)) => a.total_cmp(b),
            (Self::Text(a), Self::Text(b)) => natural(a, b),
            (Self::Num(_), Self::Text(_)) => Ordering::Less,
            (Self::Text(_), Self::Num(_)) => Ordering::Greater,
        }
    }
}

/// Text order with runs of digits compared as numbers: "v1.9" < "v1.10", "item2" < "item10".
pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        it.next();
                    }
                    digits
                };
                let (da, db) = (take(&mut a), take(&mut b));
                let (ta, tb) = (da.trim_start_matches('0'), db.trim_start_matches('0'));
                let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a.next();
                b.next();
            }
        }
    }
}

/// A sortable column: its title, width, whether it holds numbers (right-aligned, biggest first on
/// the first click), and whether it is the last column that fills the rest of the row.
#[derive(Clone, Copy, Debug)]
pub struct Col {
    pub title: &'static str,
    pub width: f32,
    pub numeric: bool,
}

pub const fn text(title: &'static str, width: f32) -> Col {
    Col {
        title,
        width,
        numeric: false,
    }
}

pub const fn num(title: &'static str, width: f32) -> Col {
    Col {
        title,
        width,
        numeric: true,
    }
}

/// The chosen (column, descending) of a table, or `None` for its natural order.
pub type Sort = Option<(usize, bool)>;

pub fn sort_items<T>(items: &mut [T], sort: Sort, key: impl Fn(&T, usize) -> SortKey) {
    if let Some((col, descending)) = sort {
        items.sort_by(|a, b| {
            let o = key(a, col).compare(&key(b, col));
            if descending { o.reverse() } else { o }
        });
    }
}

pub const MIN_WIDTH: f32 = 48.;
pub const MAX_WIDTH: f32 = 800.;
pub const HEADER_HEIGHT: f32 = 28.;
/// The side insets of a listing's header and rows, on top of its columns.
pub const TABLE_PAD: f32 = 24.;

/// A table cell of a fixed width.
pub fn fixed_cell(width: f32) -> Div {
    div().flex_shrink_0().w(px(width)).px_2().overflow_hidden()
}

/// One column of a table as the user arranged it. The cells keep the order of the page's `Col`s
/// (`index`); the layout only chooses which are shown, their order and their widths.
#[derive(Clone, Debug, PartialEq)]
pub struct ListColumn {
    pub index: usize,
    pub title: &'static str,
    pub numeric: bool,
    pub width: f32,
    pub visible: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListLayout {
    /// In display order.
    pub columns: Vec<ListColumn>,
    /// The sort: a column's title and whether it is descending. Stored by title so it survives
    /// a reordering.
    sorted: Option<(&'static str, bool)>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedColumn {
    pub title: String,
    pub width: f32,
    pub visible: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SavedTable {
    pub columns: Vec<SavedColumn>,
    pub sort: Option<(String, bool)>,
}

impl ListLayout {
    /// The layout of a table with columns `cols`, arranged as `saved` says. Columns the saved
    /// layout does not know are appended, and ones it no longer has are dropped.
    pub fn new(cols: &[Col], saved: Option<&SavedTable>) -> Self {
        let mut columns: Vec<ListColumn> = cols
            .iter()
            .enumerate()
            .map(|(index, c)| ListColumn {
                index,
                title: c.title,
                numeric: c.numeric,
                width: c.width,
                visible: true,
            })
            .collect();
        let mut sorted = None;
        if let Some(saved) = saved {
            let mut ordered = Vec::with_capacity(columns.len());
            for s in &saved.columns {
                if let Some(p) = columns.iter().position(|c| c.title == s.title.as_str()) {
                    let mut c = columns.remove(p);
                    c.width = s.width.clamp(MIN_WIDTH, MAX_WIDTH);
                    c.visible = s.visible;
                    ordered.push(c);
                }
            }
            ordered.append(&mut columns);
            columns = ordered;
            if !columns.iter().any(|c| c.visible) {
                columns.iter_mut().for_each(|c| c.visible = true);
            }
            sorted = saved.sort.as_ref().and_then(|(title, descending)| {
                cols.iter()
                    .find(|c| c.title == title.as_str())
                    .map(|c| (c.title, *descending))
            });
        }
        Self { columns, sorted }
    }

    /// The shown columns, with their display positions.
    pub fn shown(&self) -> impl Iterator<Item = (usize, &ListColumn)> {
        self.columns.iter().enumerate().filter(|(_, c)| c.visible)
    }

    pub fn shown_width(&self) -> f32 {
        self.shown().map(|(_, c)| c.width).sum()
    }

    /// The sort as a column index of the page's `Col`s.
    pub fn sort_key(&self) -> Sort {
        let (title, descending) = self.sorted?;
        self.columns
            .iter()
            .find(|c| c.title == title)
            .map(|c| (c.index, descending))
    }

    /// Sort by column `index`, or reverse the sort if it is already the sort column. Numbers
    /// start descending.
    pub fn sort_by(&mut self, index: usize, numeric: bool) {
        let Some(title) = self
            .columns
            .iter()
            .find(|c| c.index == index)
            .map(|c| c.title)
        else {
            return;
        };
        self.sorted = match self.sorted {
            Some((t, descending)) if t == title => Some((t, !descending)),
            _ => Some((title, numeric)),
        };
    }

    /// Show or hide the column at display position `pos`; the last shown column stays.
    pub fn toggle(&mut self, pos: usize) {
        let shown = self.columns.iter().filter(|c| c.visible).count();
        if let Some(c) = self.columns.get_mut(pos)
            && (!c.visible || shown > 1)
        {
            c.visible = !c.visible;
        }
    }

    pub fn set_width(&mut self, pos: usize, width: f32) {
        if let Some(c) = self.columns.get_mut(pos) {
            c.width = width.clamp(MIN_WIDTH, MAX_WIDTH);
        }
    }

    pub fn move_slot(&mut self, from: usize, slot: usize) {
        move_slot(&mut self.columns, from, slot);
    }

    fn saved(&self) -> SavedTable {
        SavedTable {
            columns: self
                .columns
                .iter()
                .map(|c| SavedColumn {
                    title: c.title.to_string(),
                    width: c.width,
                    visible: c.visible,
                })
                .collect(),
            sort: self.sorted.map(|(t, d)| (t.to_string(), d)),
        }
    }
}

/// Move the item at `from` so that it lands in front of slot `slot` (0 = first, `len` = last).
pub fn move_slot<T>(items: &mut Vec<T>, from: usize, slot: usize) {
    if from >= items.len() || slot > items.len() {
        return;
    }
    let dest = if from < slot { slot - 1 } else { slot };
    if dest != from {
        let item = items.remove(from);
        items.insert(dest, item);
    }
}

fn layouts_path() -> PathBuf {
    crate::sysops::data_dir().join("table-layouts.json")
}

fn read_saved() -> HashMap<String, SavedTable> {
    fs::read(layouts_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_saved(saved: &HashMap<String, SavedTable>) {
    let path = layouts_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(saved) {
        let _ = fs::write(path, json);
    }
}

/// A header being dragged to another column of its table.
#[derive(Clone)]
pub struct HeaderDrag {
    pub table: &'static str,
    pub from: usize,
    pub title: SharedString,
}

/// A row of the Columns menu being dragged to another place in it.
#[derive(Clone)]
pub struct MenuDrag {
    pub from: usize,
    pub title: SharedString,
}

/// The chip that follows the pointer while a column is dragged.
pub struct DragGhost(pub SharedString);

impl Render for DragGhost {
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
            .child(self.0.clone())
    }
}

/// A column being resized: its display position and where the drag started.
#[derive(Clone, Copy, Debug)]
pub struct ListResize {
    pub table: &'static str,
    pub pos: usize,
    pub start_x: Pixels,
    pub start_width: f32,
}

/// One row of a Columns menu.
pub struct ColItem {
    /// Selector suffix for the row's checkbox (`col-check-{key}`).
    pub key: String,
    pub title: &'static str,
    pub visible: bool,
}

pub fn drop_zone_row(pos: usize, top: bool) -> String {
    format!("col-drop-{pos}-{}", if top { "top" } else { "bottom" })
}

fn column_row(
    t: &Theme,
    table: &'static str,
    pos: usize,
    item: ColItem,
    cx: &mut Context<OtmApp>,
) -> impl IntoElement {
    let ColItem {
        key,
        title,
        visible,
    } = item;
    div()
        .id(("col-opt", pos))
        .debug_selector(move || format!("col-row-{pos}"))
        .relative()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded(px(8.))
        .text_sm()
        .hover(|h| h.bg(t.glass_hover))
        .on_drag(
            MenuDrag {
                from: pos,
                title: SharedString::from(title),
            },
            |d: &MenuDrag, _, _, cx| cx.new(|_| DragGhost(d.title.clone())),
        )
        .child(ui::icon("grip-vertical", 12., t.text_faint))
        .child(
            div()
                .id(("col-check", pos))
                .debug_selector(move || format!("col-check-{key}"))
                .size(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .border_1()
                .border_color(if visible { t.accent } else { t.stroke })
                .when(visible, |d| d.bg(t.selection).text_color(gpui::white()))
                .text_xs()
                .cursor_pointer()
                .children(visible.then(|| ui::icon("check", 11., gpui::white())))
                .on_click(
                    cx.listener(move |this, _, _, cx| this.toggle_menu_column(table, pos, cx)),
                ),
        )
        // The name toggles the column too: a far bigger target than the box.
        .child(
            div()
                .id(("col-name", pos))
                .flex_1()
                .cursor_pointer()
                .child(title)
                .on_click(
                    cx.listener(move |this, _, _, cx| this.toggle_menu_column(table, pos, cx)),
                ),
        )
        .child(drop_zone(t, table, pos, true, cx))
        .child(drop_zone(t, table, pos, false, cx))
}

/// The top or bottom half of a menu row: dropping there puts the dragged row above or below it.
fn drop_zone(
    t: &Theme,
    table: &'static str,
    pos: usize,
    top: bool,
    cx: &mut Context<OtmApp>,
) -> impl IntoElement {
    let accent = t.accent;
    let slot = if top { pos } else { pos + 1 };
    div()
        .id(if top {
            ("col-drop-top", pos)
        } else {
            ("col-drop-bottom", pos)
        })
        .debug_selector(move || drop_zone_row(pos, top))
        .absolute()
        .left_0()
        .right_0()
        .h(px(13.))
        .when(top, |d| d.top_0())
        .when(!top, |d| d.bottom_0())
        .drag_over::<MenuDrag>(move |s, _, _, _| {
            let s = if top { s.border_t_2() } else { s.border_b_2() };
            s.border_color(accent)
        })
        .on_drop(cx.listener(move |this, d: &MenuDrag, _, cx| {
            this.move_menu_column(table, d.from, slot, cx)
        }))
}

impl OtmApp {
    pub fn table_sort(&self, table: &str) -> Sort {
        self.table_layouts.get(table).and_then(|l| l.sort_key())
    }

    /// Make sure `table` has a layout: the saved one, or the defaults from `cols`.
    pub fn ensure_table(&mut self, table: &'static str, cols: &[Col]) {
        if table.is_empty() || self.table_layouts.contains_key(table) {
            return;
        }
        let saved = read_saved().remove(table);
        self.table_layouts
            .insert(table, ListLayout::new(cols, saved.as_ref()));
    }

    pub fn save_table_layouts(&self) {
        let mut saved = read_saved();
        for (table, layout) in &self.table_layouts {
            saved.insert(table.to_string(), layout.saved());
        }
        write_saved(&saved);
    }

    /// The width a table's shown columns take, without the listing's side insets or actions.
    pub fn table_content_width(&self, table: &str) -> f32 {
        self.table_layouts
            .get(table)
            .map_or(0., |l| l.shown_width())
    }

    /// The width of one column of a table (by its `Col` index).
    pub fn col_width(&self, table: &str, index: usize) -> f32 {
        self.table_layouts
            .get(table)
            .and_then(|l| l.columns.iter().find(|c| c.index == index))
            .map_or(0., |c| c.width)
    }

    pub fn sort_table(&mut self, table: &'static str, index: usize, numeric: bool) {
        if let Some(l) = self.table_layouts.get_mut(table) {
            l.sort_by(index, numeric);
        }
        self.save_table_layouts();
        if self.is_listing() {
            self.rebuild_listing();
        }
    }

    /// Drop the column at `from` onto the column at `target`, taking its place.
    pub fn move_table_column(&mut self, table: &'static str, from: usize, target: usize) {
        let slot = if from < target { target + 1 } else { target };
        if let Some(l) = self.table_layouts.get_mut(table) {
            l.move_slot(from, slot);
        }
        self.save_table_layouts();
    }

    /// Cells of one row in the table's display order and widths. `cells[i]` is the cell of
    /// column `i` (the page's `Col` order).
    pub fn lay_out(&self, table: &str, cells: Vec<Div>) -> Vec<gpui::AnyElement> {
        let Some(layout) = self.table_layouts.get(table) else {
            return vec![];
        };
        let mut slots: Vec<Option<Div>> = cells.into_iter().map(Some).collect();
        layout
            .shown()
            .filter_map(|(_, c)| {
                let content = slots.get_mut(c.index)?.take()?;
                Some(
                    fixed_cell(c.width)
                        .text_sm()
                        .when(c.numeric, |d| d.text_right())
                        .child(content)
                        .into_any_element(),
                )
            })
            .collect()
    }

    /// Header titles that sort, reorder, resize and auto-fit `table`'s columns, in display order.
    pub fn table_header(&self, t: &Theme, table: &'static str, cx: &mut Context<Self>) -> Div {
        let Some(layout) = self.table_layouts.get(table) else {
            return div();
        };
        let sort = layout.sort_key();
        div()
            .flex()
            .flex_row()
            .h(px(HEADER_HEIGHT))
            .items_center()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(t.text_dim)
            .children(layout.shown().map(|(pos, c)| {
                let (index, title, numeric, width) = (c.index, c.title, c.numeric, c.width);
                let active = sort.filter(|s| s.0 == index);
                div()
                    .id(SharedString::from(format!("sort-{table}-{index}")))
                    .debug_selector(move || format!("sort-{table}-{index}"))
                    .relative()
                    .flex_shrink_0()
                    .w(px(width))
                    .h_full()
                    .flex()
                    .items_center()
                    .px_2()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|h| h.bg(t.glass_hover))
                    .when(numeric, |d| d.justify_end())
                    .when(active.is_some(), |d| d.text_color(t.accent))
                    .child(div().min_w_0().truncate().child(title))
                    .children(active.map(|(_, descending)| {
                        let name = if descending { "arrow-down" } else { "arrow-up" };
                        div()
                            .flex_none()
                            .pl_1()
                            .child(ui::icon(name, 10., t.accent))
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.sort_table(table, index, numeric);
                        cx.notify();
                    }))
                    .on_drag(
                        HeaderDrag {
                            table,
                            from: pos,
                            title: SharedString::from(title),
                        },
                        |d: &HeaderDrag, _, _, cx| cx.new(|_| DragGhost(d.title.clone())),
                    )
                    .drag_over::<HeaderDrag>(|s, _, _, _| s.border_l_2())
                    .on_drop(cx.listener(move |this, d: &HeaderDrag, _, cx| {
                        if d.table == table {
                            this.move_table_column(table, d.from, pos);
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .id(SharedString::from(format!("resize-{table}-{index}")))
                            .debug_selector(move || format!("resize-{table}-{index}"))
                            .absolute()
                            .right(px(-3.))
                            .top_1()
                            .bottom_1()
                            .w(px(7.))
                            .cursor_col_resize()
                            .child(div().mx_auto().w(px(1.)).h_full().bg(t.stroke))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    if ev.click_count > 1 {
                                        this.fit_list_column(table, pos, window, cx);
                                    } else {
                                        this.list_resizing = Some(ListResize {
                                            table,
                                            pos,
                                            start_x: ev.position.x,
                                            start_width: width,
                                        });
                                    }
                                }),
                            ),
                    )
            }))
            .child(div().flex_1().min_w_0())
    }

    /// The horizontal scroll of a table body: its width is `width`, and a bar shows when that
    /// is wider than the space. `(id, bar, fill)`: with `fill`, the body takes the space of its
    /// positioned parent.
    pub fn scroll_x(
        &self,
        t: &Theme,
        (id, bar, fill): (&'static str, &'static str, bool),
        handle: &ScrollHandle,
        width: f32,
        body: Div,
        cx: &mut Context<Self>,
    ) -> Div {
        let area = div()
            .id(id)
            .w_full()
            .overflow_x_scroll()
            .track_scroll(handle)
            .child(
                div()
                    .w(px(width))
                    .min_w_full()
                    .when(fill, |d| d.h_full())
                    .child(body),
            );
        let area = if fill {
            area.absolute().inset_0()
        } else {
            area
        };
        div()
            .relative()
            .w_full()
            .min_w_0()
            .when(fill, |d| d.flex_1().min_h_0())
            .child(axis_locked(area))
            .when_some(
                scrollbar::scrollbar(t, bar, Axis::Horizontal, handle, cx),
                |d, b| d.child(b),
            )
    }

    /// The menu rows of a table: the processes' columns, or a listing's or Disk's.
    pub fn menu_items(&self, table: &'static str) -> Vec<ColItem> {
        if table == "processes" {
            self.layout
                .columns
                .iter()
                .map(|c| ColItem {
                    key: format!("{:?}", c.id),
                    title: c.id.title(),
                    visible: c.visible,
                })
                .collect()
        } else {
            self.table_layouts
                .get(table)
                .map(|l| {
                    l.columns
                        .iter()
                        .map(|c| ColItem {
                            key: c.title.to_string(),
                            title: c.title,
                            visible: c.visible,
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
    }

    /// The Columns menu of `table`: a checkbox to show or hide each column, a grip to drag it
    /// to another place, and a reset. The caller places it.
    pub fn columns_menu(
        &self,
        t: &Theme,
        table: &'static str,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let items = self.menu_items(table);
        div()
            .id("column-menu")
            .debug_selector(|| "column-menu".into())
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
            .children(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(pos, item)| column_row(t, table, pos, item, cx).into_any_element()),
            )
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
                    .on_click(cx.listener(move |this, _, _, cx| this.reset_columns(table, cx))),
            )
    }

    /// The Columns button of a listing or Disk table, with its menu under it when open.
    pub fn columns_button(&self, t: &Theme, table: &'static str, cx: &mut Context<Self>) -> Div {
        let open = self.column_menu && self.column_menu_table == table;
        div()
            .relative()
            .flex_shrink_0()
            .child(
                ui::pill_menu(
                    t,
                    SharedString::from(format!("columns-{table}")),
                    "Columns",
                    open,
                )
                .debug_selector(move || format!("columns-{table}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        // A press here while open already closed the menu as a click outside
                        // it; don't reopen it straight away.
                        if !std::mem::take(&mut this.column_menu_just_closed) {
                            this.column_menu = !open;
                            this.column_menu_table = table;
                        }
                        cx.notify();
                    }),
                ),
            )
            .when(open, |d| {
                d.child(
                    self.columns_menu(t, table, cx)
                        .absolute()
                        .top_full()
                        .right_0()
                        .mt_1(),
                )
            })
    }

    /// Show, hide or move a column from the Columns menu of `table`.
    pub fn toggle_menu_column(&mut self, table: &'static str, pos: usize, cx: &mut Context<Self>) {
        if table == "processes" {
            if let Some(id) = self.layout.columns.get(pos).map(|c| c.id) {
                self.layout.toggle(id);
            }
            self.layout_changed(cx);
        } else {
            if let Some(l) = self.table_layouts.get_mut(table) {
                l.toggle(pos);
            }
            self.save_table_layouts();
            cx.notify();
        }
    }

    pub fn move_menu_column(
        &mut self,
        table: &'static str,
        from: usize,
        slot: usize,
        cx: &mut Context<Self>,
    ) {
        if table == "processes" {
            move_slot(&mut self.layout.columns, from, slot);
            self.layout_changed(cx);
        } else {
            if let Some(l) = self.table_layouts.get_mut(table) {
                l.move_slot(from, slot);
            }
            self.save_table_layouts();
            cx.notify();
        }
    }

    /// Back to the default columns, widths and sort of `table`.
    pub fn reset_columns(&mut self, table: &'static str, cx: &mut Context<Self>) {
        if table == "processes" {
            let group = self.layout.group_by;
            self.layout = Default::default();
            self.layout.group_by = group;
            self.layout_changed(cx);
        } else {
            self.table_layouts.remove(table);
            let mut saved = read_saved();
            saved.remove(table);
            write_saved(&saved);
            self.rebuild_listing();
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order_compares_digit_runs_as_numbers() {
        assert_eq!(natural("v1.9", "v1.10"), Ordering::Less);
        assert_eq!(natural("item10", "item2"), Ordering::Greater);
        assert_eq!(natural("a", "b"), Ordering::Less);
        assert_eq!(natural("007", "7"), Ordering::Equal);
    }

    #[test]
    fn numbers_sort_by_value_not_text() {
        let mut v = vec![(9 * 1024 * 1024) as f64, (10 * 1024 * 1024) as f64, 512.];
        sort_items(&mut v, Some((0, false)), |x, _| SortKey::num(*x));
        assert_eq!(
            v,
            vec![512., (9 * 1024 * 1024) as f64, (10 * 1024 * 1024) as f64]
        );
        sort_items(&mut v, Some((0, true)), |x, _| SortKey::num(*x));
        assert_eq!(v[0], (10 * 1024 * 1024) as f64);
    }

    #[test]
    fn move_slot_puts_the_item_in_front_of_the_slot() {
        let mut v = vec!['a', 'b', 'c', 'd'];
        move_slot(&mut v, 0, 3);
        assert_eq!(v, ['b', 'c', 'a', 'd']);
        move_slot(&mut v, 3, 0);
        assert_eq!(v, ['d', 'b', 'c', 'a']);
        move_slot(&mut v, 1, 1);
        move_slot(&mut v, 1, 2);
        assert_eq!(v, ['d', 'b', 'c', 'a']);
    }

    #[test]
    fn saved_layout_keeps_order_widths_visibility_and_sort() {
        let cols = [text("A", 100.), num("B", 90.), text("C", 80.)];
        let mut layout = ListLayout::new(&cols, None);
        layout.move_slot(0, 3);
        layout.set_width(2, 200.);
        layout.toggle(0);
        layout.sort_by(1, true);
        let back = ListLayout::new(&cols, Some(&layout.saved()));
        assert_eq!(back, layout);
        assert_eq!(back.sort_key(), Some((1, true)));
    }

    #[test]
    fn new_columns_are_appended_and_dropped_ones_ignored() {
        let saved = SavedTable {
            columns: vec![
                SavedColumn {
                    title: "C".into(),
                    width: 50.,
                    visible: true,
                },
                SavedColumn {
                    title: "gone".into(),
                    width: 50.,
                    visible: true,
                },
            ],
            sort: Some(("gone".into(), true)),
        };
        let cols = [text("A", 100.), text("C", 80.)];
        let layout = ListLayout::new(&cols, Some(&saved));
        let titles: Vec<&str> = layout.columns.iter().map(|c| c.title).collect();
        assert_eq!(titles, ["C", "A"]);
        assert_eq!(layout.columns[0].width, 50.);
        assert_eq!(layout.sort_key(), None);
    }

    #[test]
    fn at_least_one_column_stays_visible() {
        let cols = [text("A", 100.), text("B", 80.)];
        let mut layout = ListLayout::new(&cols, None);
        layout.toggle(0);
        layout.toggle(1);
        assert_eq!(layout.shown().count(), 1);
    }
}
