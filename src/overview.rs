use gpui::{Div, FontWeight, div, prelude::*, px};

use crate::{
    app::OtmApp,
    metrics::{HISTORY, fmt_bytes, fmt_duration},
    theme::Theme,
    ui,
};

fn card(t: &Theme, title: &'static str, value: String, detail: String) -> Div {
    div()
        .flex_1()
        .min_w(px(220.))
        .p_4()
        .flex()
        .flex_col()
        .gap_2()
        .rounded(px(18.))
        .bg(t.glass_strong)
        .border_1()
        .border_color(t.highlight)
        .shadow_sm()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.text_dim)
                .child(title),
        )
        .child(div().text_2xl().font_weight(FontWeight::BOLD).child(value))
        .child(div().text_xs().text_color(t.text_dim).child(detail))
}

impl OtmApp {
    pub fn render_overview(&self, t: &Theme) -> impl IntoElement {
        let s = &self.snapshot;
        let h = &self.history;
        let mem_pct = if s.memory_total == 0 {
            0.
        } else {
            s.memory_used as f32 / s.memory_total as f32 * 100.
        };
        let net_max = h
            .net_down
            .iter()
            .chain(h.net_up.iter())
            .copied()
            .fold(1024., f32::max);
        let boot_disk = s
            .disks
            .iter()
            .find(|d| d.mount == "/")
            .or_else(|| s.disks.first())
            .cloned()
            .unwrap_or_default();
        let disk_pct = if boot_disk.total == 0 {
            0.
        } else {
            (boot_disk.total - boot_disk.available) as f32 / boot_disk.total as f32 * 100.
        };

        ui::glass(t)
            .id("overview")
            .size_full()
            .p_4()
            .flex()
            .flex_col()
            .gap_4()
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::BOLD)
                            .child("Overview"),
                    )
                    .child(div().text_xs().text_color(t.text_dim).child(format!(
                        "{} · {} · up {}",
                        s.os,
                        s.cpu_brand,
                        fmt_duration(s.uptime)
                    ))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_3()
                    .child(
                        card(
                            t,
                            "CPU",
                            format!("{:.0}%", s.cpu),
                            format!("{} cores", s.cores.len()),
                        )
                        .child(ui::sparkline(
                            h.cpu.iter().copied(),
                            100.,
                            54.,
                            t.load_color(s.cpu),
                            HISTORY,
                        )),
                    )
                    .child(
                        card(
                            t,
                            "MEMORY",
                            format!("{mem_pct:.0}%"),
                            format!(
                                "{} of {} · swap {}",
                                fmt_bytes(s.memory_used),
                                fmt_bytes(s.memory_total),
                                fmt_bytes(s.swap_used)
                            ),
                        )
                        .child(ui::sparkline(
                            h.memory.iter().copied(),
                            100.,
                            54.,
                            t.load_color(mem_pct),
                            HISTORY,
                        )),
                    )
                    .child(
                        card(
                            t,
                            "NETWORK",
                            format!("↓ {}/s", fmt_bytes(s.net_down)),
                            format!("↑ {}/s", fmt_bytes(s.net_up)),
                        )
                        .child(ui::sparkline(
                            h.net_down.iter().copied(),
                            net_max,
                            26.,
                            t.accent.into(),
                            HISTORY,
                        ))
                        .child(ui::sparkline(
                            h.net_up.iter().copied(),
                            net_max,
                            26.,
                            t.good.into(),
                            HISTORY,
                        )),
                    )
                    .child(
                        card(
                            t,
                            "DISK",
                            format!("{disk_pct:.0}%"),
                            format!(
                                "{} free of {}",
                                fmt_bytes(boot_disk.available),
                                fmt_bytes(boot_disk.total)
                            ),
                        )
                        .child(ui::meter(
                            t,
                            disk_pct,
                            t.load_color(disk_pct),
                        )),
                    ),
            )
            .child({
                let i = &self.system;
                let fact = |k: &'static str, v: String| {
                    div()
                        .w(px(300.))
                        .flex()
                        .flex_row()
                        .text_sm()
                        .child(div().w(px(130.)).text_color(t.text_dim).child(k))
                        .child(div().truncate().child(v))
                };
                let booted = crate::sysops::now_secs().saturating_sub(i.booted);
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .rounded(px(18.))
                    .bg(t.glass_strong)
                    .border_1()
                    .border_color(t.highlight)
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.text_dim)
                            .child("SYSTEM INFO"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap_x_6()
                            .gap_y_1()
                            .child(fact("Model", i.model.clone()))
                            .child(fact("Chip", i.chip.clone()))
                            .child(fact("macOS", format!("{} ({})", i.os, i.build)))
                            .child(fact("Architecture", i.arch.clone()))
                            .child(fact("Memory", fmt_bytes(i.memory)))
                            .child(fact(
                                "Cores",
                                format!(
                                    "{} ({} performance, {} efficiency)",
                                    i.physical_cores, i.performance_cores, i.efficiency_cores
                                ),
                            ))
                            .child(fact("Host", s.host.clone()))
                            .child(fact(
                                "Booted",
                                if i.booted == 0 {
                                    "—".into()
                                } else {
                                    format!("{} ago", fmt_duration(booted))
                                },
                            )),
                    )
            })
            .child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .rounded(px(18.))
                    .bg(t.glass_strong)
                    .border_1()
                    .border_color(t.highlight)
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.text_dim)
                            .child("PER-CORE LOAD"),
                    )
                    .child(div().flex().flex_row().flex_wrap().gap_3().children(
                        s.cores.iter().enumerate().map(|(i, &pct)| {
                            div()
                                .w(px(120.))
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .justify_between()
                                        .text_xs()
                                        .text_color(t.text_dim)
                                        .child(format!("Core {}", i + 1))
                                        .child(format!("{pct:.0}%")),
                                )
                                .child(ui::meter(t, pct, t.load_color(pct)))
                        }),
                    )),
            )
    }
}
