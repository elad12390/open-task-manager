use gpui::{AnyWindowHandle, TestAppContext, VisualTestContext};

use crate::{app::OtmApp, ui_tests::ENV_LOCK};

fn data_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("otm-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };
    dir
}

fn secondary_windows(cx: &VisualTestContext) -> Vec<AnyWindowHandle> {
    cx.windows()
}

fn close(cx: &mut VisualTestContext, handle: AnyWindowHandle) {
    handle
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
}

#[gpui::test]
fn popout_opens_once_and_reopens_after_close(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("popout-test");

    let (_view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    let main = secondary_windows(cx)[0];

    crate::ui_tests::click(cx, "open-popout");
    let opened = secondary_windows(cx);
    assert_eq!(opened.len(), 2, "clicking Popout should open one window");

    crate::ui_tests::click(cx, "open-popout");
    assert_eq!(
        secondary_windows(cx).len(),
        2,
        "a second Popout click should focus the existing window, not open another"
    );

    let popout = *secondary_windows(cx).iter().find(|h| **h != main).unwrap();
    close(cx, popout);
    assert_eq!(secondary_windows(cx).len(), 1, "closing removes the popout");

    crate::ui_tests::click(cx, "open-popout");
    assert_eq!(
        secondary_windows(cx).len(),
        2,
        "Popout should reopen after it was closed"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[gpui::test]
fn screensaver_opens_from_button_and_f12_then_closes(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("saver-test");

    let (_view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    let main = secondary_windows(cx)[0];

    crate::ui_tests::click(cx, "open-saver");
    assert_eq!(
        secondary_windows(cx).len(),
        2,
        "Screensaver button should open the saver"
    );

    crate::ui_tests::click(cx, "open-saver");
    assert_eq!(
        secondary_windows(cx).len(),
        2,
        "a second Screensaver click should not open a duplicate"
    );

    let saver = *secondary_windows(cx).iter().find(|h| **h != main).unwrap();
    close(cx, saver);
    assert_eq!(secondary_windows(cx).len(), 1, "closing removes the saver");

    main.update(cx, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    cx.simulate_keystrokes("f12");
    cx.run_until_parked();
    assert_eq!(
        secondary_windows(cx).len(),
        2,
        "F12 should open the screensaver"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[gpui::test]
fn screensaver_closes_on_escape_and_reopens(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("saver-esc-test");

    let (_view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    let main = secondary_windows(cx)[0];

    crate::ui_tests::click(cx, "open-saver");
    let saver = *secondary_windows(cx).iter().find(|h| **h != main).unwrap();
    saver
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    cx.cx.simulate_keystrokes(saver, "escape");
    cx.run_until_parked();
    assert_eq!(secondary_windows(cx).len(), 1, "Esc should close the saver");

    crate::ui_tests::click(cx, "open-saver");
    assert_eq!(
        secondary_windows(cx).len(),
        2,
        "Screensaver should reopen after Esc closed it"
    );

    std::fs::remove_dir_all(&dir).ok();
}
