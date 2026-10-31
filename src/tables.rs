//! Sortable tables outside the process list: sort keys, each table's sort, and a header row whose
//! titles sort it (click to sort, click again to reverse; numbers start biggest first).

use std::cmp::Ordering;

use gpui::{Context, Div, FontWeight, SharedString, div, prelude::*, px};

use crate::{app::OtmApp, theme::Theme};

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

/// The width a table cell gets: fixed, or the rest of the row for the last column.
pub fn cell(last: bool, width: f32) -> Div {
    let d = div().px_2().overflow_hidden();
    if last {
        d.flex_1().min_w_0()
    } else {
        d.flex_shrink_0().w(px(width))
    }
}

impl OtmApp {
    pub fn table_sort(&self, table: &str) -> Sort {
        self.table_sorts.get(table).copied()
    }

    /// Sort by `col`, or reverse it if it is already the sort column. Numbers start descending.
    pub fn toggle_sort(&mut self, table: &'static str, col: usize, numeric: bool) {
        let next = match self.table_sorts.get(table) {
            Some(&(c, descending)) if c == col => (col, !descending),
            _ => (col, numeric),
        };
        self.table_sorts.insert(table, next);
    }

    /// Header titles that sort `table`. With `last_fills`, the last column takes the rest of the row.
    pub fn sort_header(
        &self,
        t: &Theme,
        table: &'static str,
        cols: &[Col],
        last_fills: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let sort = self.table_sort(table);
        div()
            .flex()
            .flex_row()
            .items_center()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(t.text_dim)
            .children(cols.iter().copied().enumerate().map(|(i, c)| {
                let active = sort.filter(|s| s.0 == i);
                let last = last_fills && i + 1 == cols.len();
                cell(last, c.width)
                    .id(SharedString::from(format!("sort-{table}-{i}")))
                    .debug_selector(move || format!("sort-{table}-{i}"))
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .when(c.numeric, |d| d.justify_end())
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|h| h.bg(t.glass_hover))
                    .when(active.is_some(), |d| d.text_color(t.accent))
                    .child(c.title)
                    .children(active.map(|(_, descending)| {
                        let name = if descending { "arrow-down" } else { "arrow-up" };
                        div()
                            .flex_none()
                            .pl_1()
                            .child(crate::ui::icon(name, 10., t.accent))
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_sort(table, i, c.numeric);
                        this.rebuild_listing();
                        cx.notify();
                    }))
            }))
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
}
