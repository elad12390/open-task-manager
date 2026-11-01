mod app;
mod assets;
mod categories;
mod column_fit;
mod columns;
mod context_menu;
#[cfg(test)]
mod copy_pass_tests;
#[cfg(test)]
mod crash_fix_tests;
mod demo;
#[cfg(test)]
mod demo_linux_tests;
mod filter;
mod filter_bar;
#[cfg(test)]
mod holiday_themes_tests;
mod holidays;
#[cfg(test)]
mod icon_pack_tests;
mod icons;
#[cfg(test)]
mod linux_apps_tests;
#[cfg(test)]
mod linux_flatpak_tests;
#[cfg(test)]
mod listing_autocomplete_tests;
mod menus;
mod metrics;
#[cfg(test)]
mod multiselect_tests;
mod overview;
#[cfg(test)]
mod page_scroll_tests;
mod pages;
mod platform;
#[cfg(test)]
mod platform_split_tests;
mod processes;
mod recorder;
mod scrollbar;
#[cfg(test)]
mod select_anything_tests;
mod selftest;
mod settings;
mod sysops;
#[cfg(test)]
mod table_icons_tests;
#[cfg(test)]
mod table_layout_tests;
#[cfg(test)]
mod table_ux_tests;
mod tables;
mod theme;
mod ui;
#[cfg(test)]
mod ui_tests;
mod windows;

use gpui::{
    App, Application, Bounds, WindowBounds, WindowKind, WindowOptions, prelude::*, px, size,
};

/// Prints GPUI's and our log records to stderr when `OTM_LOG` is set (e.g. `OTM_LOG=info`).
struct StderrLog;

impl log::Log for StderrLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}: {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

fn init_log() {
    let Some(level) = std::env::var("OTM_LOG")
        .ok()
        .and_then(|l| l.parse::<log::LevelFilter>().ok())
    else {
        return;
    };
    if log::set_logger(&StderrLog).is_ok() {
        log::set_max_level(level);
    }
}

fn main() {
    init_log();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--demo") {
        let scene = args
            .get(i + 1)
            .cloned()
            .unwrap_or_else(|| "overview".into());
        if !demo::SCENES.contains(&scene.as_str()) {
            eprintln!(
                "unknown demo scene {scene:?}; choose one of {:?}",
                demo::SCENES
            );
            std::process::exit(2);
        }
        // SAFETY: single-threaded, before the app or any other thread starts.
        unsafe { std::env::set_var("OTM_DEMO", scene) };
    }
    if std::env::args().any(|a| a == "--version") {
        println!("otm {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if std::env::args().any(|a| a == "--selftest") {
        std::process::exit(selftest::run());
    }
    // Test runs set OTM_BACKGROUND=1 so the window never activates or takes keyboard focus.
    let background = std::env::var_os("OTM_BACKGROUND").is_some();
    Application::new()
        .with_assets(assets::Assets)
        .run(move |cx: &mut App| {
            menus::install(cx);
            // `OTM_WINDOW_SIZE=WxH` sizes the window for captures (the capture script wants a wider one).
            let (w, h) = std::env::var("OTM_WINDOW_SIZE")
                .ok()
                .and_then(|s| {
                    let (w, h) = s.split_once('x')?;
                    Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?))
                })
                .unwrap_or((1240., 800.));
            let bounds = Bounds::centered(None, size(px(w), px(h)), cx);
            let chrome = platform::window_chrome(platform::WindowRole::Main);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: chrome.titlebar,
                    window_background: chrome.background,
                    window_min_size: Some(size(px(860.), px(520.))),
                    app_id: Some(platform::APP_ID.into()),
                    focus: !background,
                    // A covered window stops drawing, so test runs float above others without taking focus.
                    kind: if background {
                        WindowKind::PopUp
                    } else {
                        WindowKind::Normal
                    },
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| app::OtmApp::new(window, cx)),
            )
            .unwrap();
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            if !background {
                cx.activate(true);
            }
        });
}
