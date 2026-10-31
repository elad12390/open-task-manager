//! Icons (Lucide, ISC) and the app logo, compiled into the binary so the packaged app needs no files
//! beside it. Add an icon by copying its SVG into `assets/icons/` and listing it in `ICONS`.

use std::{borrow::Cow, sync::Arc, sync::OnceLock};

use gpui::{AssetSource, Image, ImageFormat, Result, SharedString};

pub const ICONS: &[(&str, &[u8])] = &[
    (
        "icons/app-window.svg",
        include_bytes!("../assets/icons/app-window.svg"),
    ),
    (
        "icons/arrow-down.svg",
        include_bytes!("../assets/icons/arrow-down.svg"),
    ),
    (
        "icons/arrow-up.svg",
        include_bytes!("../assets/icons/arrow-up.svg"),
    ),
    (
        "icons/check.svg",
        include_bytes!("../assets/icons/check.svg"),
    ),
    (
        "icons/chevron-down.svg",
        include_bytes!("../assets/icons/chevron-down.svg"),
    ),
    (
        "icons/chevron-right.svg",
        include_bytes!("../assets/icons/chevron-right.svg"),
    ),
    (
        "icons/circle-dot.svg",
        include_bytes!("../assets/icons/circle-dot.svg"),
    ),
    (
        "icons/circle.svg",
        include_bytes!("../assets/icons/circle.svg"),
    ),
    (
        "icons/gauge.svg",
        include_bytes!("../assets/icons/gauge.svg"),
    ),
    (
        "icons/hard-drive.svg",
        include_bytes!("../assets/icons/hard-drive.svg"),
    ),
    ("icons/list.svg", include_bytes!("../assets/icons/list.svg")),
    (
        "icons/network.svg",
        include_bytes!("../assets/icons/network.svg"),
    ),
    (
        "icons/power.svg",
        include_bytes!("../assets/icons/power.svg"),
    ),
    (
        "icons/refresh-cw.svg",
        include_bytes!("../assets/icons/refresh-cw.svg"),
    ),
    (
        "icons/search.svg",
        include_bytes!("../assets/icons/search.svg"),
    ),
    (
        "icons/settings.svg",
        include_bytes!("../assets/icons/settings.svg"),
    ),
    (
        "icons/sliders-horizontal.svg",
        include_bytes!("../assets/icons/sliders-horizontal.svg"),
    ),
    (
        "icons/square.svg",
        include_bytes!("../assets/icons/square.svg"),
    ),
    (
        "icons/timer.svg",
        include_bytes!("../assets/icons/timer.svg"),
    ),
    (
        "icons/triangle-alert.svg",
        include_bytes!("../assets/icons/triangle-alert.svg"),
    ),
    (
        "icons/users.svg",
        include_bytes!("../assets/icons/users.svg"),
    ),
    ("icons/x.svg", include_bytes!("../assets/icons/x.svg")),
    ("icons/zap.svg", include_bytes!("../assets/icons/zap.svg")),
];

const LOGO_PNG: &[u8] = include_bytes!("../assets/AppIcon.png");

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}

/// The app logo, decoded once: an image's cache is keyed by its identity, so a new one per frame
/// would be decoded again on every render.
pub fn logo() -> Arc<Image> {
    static LOGO: OnceLock<Arc<Image>> = OnceLock::new();
    LOGO.get_or_init(|| Arc::new(Image::from_bytes(ImageFormat::Png, LOGO_PNG.to_vec())))
        .clone()
}
