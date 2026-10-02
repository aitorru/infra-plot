//! infra-plot desktop: a native editor for infra-plot diagrams, built on gpui.
//!
//! Opens and saves the same JSON/TOML documents as the web editor, in a window without
//! system decorations (it draws its own title bar).

// Colours are written like their CSS counterparts (`0xf8f9fa`).
#![allow(clippy::unreadable_literal)]

mod app;
mod assets;
mod icons;
mod input;
mod paint;
mod settings;
mod state;
mod theme;

use std::path::PathBuf;

use clap::Parser;
use gpui::{
    App, AppContext, Bounds, WindowBackgroundAppearance, WindowBounds, WindowDecorations,
    WindowOptions, px, size,
};
use infraplot_model::Diagram;

use crate::app::InfraPlot;
use crate::settings::Settings;
use crate::theme::ThemeName;

#[derive(Debug, Parser)]
#[command(name = "infra-plot", version, about)]
struct Args {
    /// Diagram to open (`.json` or `.toml`). Created when it doesn't exist yet.
    file: Option<PathBuf>,
    /// Colour theme: light, dark, solarized or nier (remembered between runs).
    #[arg(long, value_parser = parse_theme)]
    theme: Option<ThemeName>,
}

fn parse_theme(s: &str) -> Result<ThemeName, String> {
    ThemeName::parse(s).ok_or_else(|| "expected light, dark, solarized or nier".into())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let settings = Settings::load();
    let theme = args
        .theme
        .or_else(|| settings.theme.as_deref().and_then(ThemeName::parse))
        .unwrap_or(ThemeName::Light);

    // Read the file before opening a window, so a bad path fails on the terminal.
    let (doc, path) = match args.file {
        Some(p) if p.exists() => (Diagram::load(&p)?, Some(p)),
        Some(p) => (Diagram::new(title_from(&p)), Some(p)),
        None => (Diagram::new("Untitled diagram"), None),
    };

    gpui_platform::application()
        .with_assets(assets::Assets)
        .run(move |cx: &mut App| {
            if let Err(e) = assets::load_fonts(cx) {
                eprintln!("infra-plot: could not load the bundled fonts: {e}");
            }
            input::bind_keys(cx);
            app::bind_keys(cx);
            let bounds = Bounds::centered(None, size(px(1400.), px(880.)), cx);
            let window = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: None,
                    window_decorations: Some(WindowDecorations::Client),
                    window_background: WindowBackgroundAppearance::Transparent,
                    window_min_size: Some(size(px(720.), px(480.))),
                    app_id: Some("infra-plot".into()),
                    app_owns_titlebar_drag: true,
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| InfraPlot::new(doc, path, theme, window, cx)),
            );
            if let Err(e) = window {
                eprintln!("infra-plot: could not open a window: {e:#}");
                cx.quit();
                return;
            }
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
    Ok(())
}

fn title_from(p: &std::path::Path) -> String {
    p.file_stem().map_or_else(
        || "Untitled diagram".into(),
        |s| s.to_string_lossy().into_owned(),
    )
}
