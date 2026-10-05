//! Embedded assets: node icons, UI glyphs and the Inter font. Everything is compiled into
//! the binary so `cargo install` gives a self-contained app.

use std::borrow::Cow;

use gpui::{App, AssetSource, SharedString};
use infraplot_model::NodeKind;

use crate::icons::icon;

pub struct Assets;

macro_rules! svg {
    ($body:literal) => {
        concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="#fff" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">"##,
            $body,
            "</svg>"
        )
    };
}

/// Stroke-only UI glyphs on a 24-unit grid, tinted by gpui like the node icons.
fn ui_svg(name: &str) -> Option<&'static str> {
    Some(match name {
        "close" => svg!(r##"<path d="M6 6l12 12M18 6L6 18"/>"##),
        "chevron-down" => svg!(r##"<path d="M6 9l6 6 6-6"/>"##),
        "chevron-up" => svg!(r##"<path d="M6 15l6-6 6 6"/>"##),
        "minimize" => svg!(r##"<path d="M6 12h12"/>"##),
        "maximize" => svg!(r##"<rect x="6" y="6" width="12" height="12" rx="1.5"/>"##),
        "restore" => svg!(
            r##"<rect x="5" y="9" width="10" height="10" rx="1.5"/><path d="M9 9V6.5A1.5 1.5 0 0 1 10.5 5h7A1.5 1.5 0 0 1 19 6.5v7a1.5 1.5 0 0 1-1.5 1.5H15"/>"##
        ),
        "select" => {
            svg!(r##"<path d="M5 3.5l13 7.5-5.6 1.4L10 18z"/><path d="M12.5 12.5l4.5 6"/>"##)
        }
        "hand" => svg!(
            r##"<path d="M8 13V5.5a1.5 1.5 0 0 1 3 0V12M11 11V4.5a1.5 1.5 0 0 1 3 0V12M14 11V6a1.5 1.5 0 0 1 3 0v8a7 7 0 0 1-7 7h-.6a6 6 0 0 1-4.6-2.2L3 15.5a1.6 1.6 0 0 1 2.4-2.1L8 16"/>"##
        ),
        "connect" => svg!(
            r##"<circle cx="5.5" cy="18.5" r="2.5"/><circle cx="18.5" cy="5.5" r="2.5"/><path d="M7.5 16.5l9-9"/>"##
        ),
        "line" => svg!(r##"<path d="M4 18h6V6h10"/><path d="M17 3l3 3-3 3"/>"##),
        "note" => svg!(r##"<path d="M5 6V4h14v2M12 4v16M9 20h6"/>"##),
        "zone" => svg!(
            r##"<rect x="3.5" y="5" width="17" height="14" rx="2.5" stroke-dasharray="3 2.5"/>"##
        ),
        "undo" => svg!(r##"<path d="M9 14L4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>"##),
        "redo" => {
            svg!(r##"<path d="M15 14l5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H13"/>"##)
        }
        "fit" => svg!(
            r##"<path d="M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5"/><rect x="9" y="9" width="6" height="6" rx="1"/>"##
        ),
        "theme" => svg!(
            r##"<circle cx="12" cy="12" r="8"/><path d="M12 4a8 8 0 0 1 0 16z" fill="#fff"/>"##
        ),
        "logo" => svg!(
            r##"<rect x="3" y="3" width="8" height="8" rx="2"/><rect x="13" y="13" width="8" height="8" rx="2"/><path d="M11 7h4a2 2 0 0 1 2 2v4"/>"##
        ),
        _ => return None,
    })
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        if let Some(name) = path
            .strip_prefix("ui/")
            .and_then(|p| p.strip_suffix(".svg"))
        {
            return Ok(ui_svg(name).map(|s| Cow::Borrowed(s.as_bytes())));
        }
        if let Some(file) = path.strip_prefix("icons/") {
            let (slug, body) = match file.strip_suffix(".body.svg") {
                Some(slug) => (slug, true),
                None => (file.strip_suffix(".svg").unwrap_or(file), false),
            };
            let found = NodeKind::ALL
                .iter()
                .find(|k| k.slug() == slug)
                .and_then(|&k| icon(k));
            return Ok(found.map(|i| Cow::Borrowed(if body { i.body } else { i.strokes })));
        }
        Ok(None)
    }

    fn list(&self, _path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

/// Registers Inter, the typeface of the web editor's `clean` look and UI.
pub fn load_fonts(cx: &App) -> gpui::Result<()> {
    cx.text_system().add_fonts(vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Medium.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-SemiBold.ttf").as_slice()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_resolve() {
        assert!(Assets.load("ui/close.svg").unwrap().is_some());
        assert!(Assets.load("icons/load-balancer.svg").unwrap().is_some());
        assert!(Assets.load("icons/database.body.svg").unwrap().is_some());
        assert!(Assets.load("icons/nope.svg").unwrap().is_none());
    }
}
