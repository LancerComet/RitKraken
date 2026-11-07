#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

extern crate gpui_kit as gpui;

mod app;
mod i18n;

use std::path::PathBuf;

use gpui_kit::*;

rust_i18n::i18n!("locales", fallback = "en");

fn main() {
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            i18n::init();
            gpui_kit::init(cx);
            ritkraken_diff::init(cx);
            app::init(cx);
            let bounds = Bounds::centered(None, size(px(1480.), px(920.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(920.), px(560.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("RitKraken".into()),
                    ..Default::default()
                }),
                app_id: Some("RitKraken".into()),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| app::Client::new(path, window, cx))
            })
            .expect("Could not open the RitKraken window");
            cx.activate(true);
        });
}
