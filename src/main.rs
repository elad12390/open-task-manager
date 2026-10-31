mod app;
mod categories;
mod columns;
mod context_menu;
#[cfg(test)]
mod crash_fix_tests;
mod demo;
mod filter;
mod filter_bar;
mod icons;
#[cfg(test)]
mod listing_autocomplete_tests;
mod menus;
mod metrics;
mod overview;
mod pages;
mod processes;
mod recorder;
mod selftest;
mod settings;
mod sysops;
#[cfg(test)]
mod table_icons_tests;
mod tables;
mod theme;
mod ui;
#[cfg(test)]
mod ui_tests;
mod windows;

use gpui::{
    App, Application, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds,
    WindowKind, WindowOptions, point, prelude::*, px, size,
};

fn main() {
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
    if std::env::args().any(|a| a == "--selftest") {
        std::process::exit(selftest::run());
    }
    // Test runs set OTM_BACKGROUND=1 so the window never activates or takes keyboard focus.
    let background = std::env::var_os("OTM_BACKGROUND").is_some();
    Application::new().run(move |cx: &mut App| {
        menus::install(cx);
        let bounds = Bounds::centered(None, size(px(1240.), px(800.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Open Task Manager".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(26.), px(26.))),
                }),
                window_background: WindowBackgroundAppearance::Blurred,
                window_min_size: Some(size(px(860.), px(520.))),
                app_id: Some("org.opentaskmanager.native".into()),
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
