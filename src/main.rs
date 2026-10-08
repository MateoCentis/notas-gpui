// Sin consola de fondo en las builds de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Las macros de GPUI generan rutas `gpui::...`; gpui-kit recomienda este alias.
extern crate gpui_kit as gpui;

mod app;
mod keymap;
mod lines;
mod notes;
mod palette;
mod platform;
mod settings;
mod settings_panel;
mod tasks;
mod theme;

use std::{borrow::Cow, path::PathBuf};

use gpui::{
    AppContext as _, AssetSource, Bounds, SharedString, WindowBounds, WindowOptions, point, px,
    size,
};
use gpui_kit::component::TitleBar;

use crate::{app::Notas, settings::Settings};

// Íconos que no vienen en el paquete por defecto de los componentes.
gpui_kit::assets::icon_assets!(ExtraIcons, [Pin, PinOff, Eye, Pencil, Sun, Moon]);

const LOGO_PNG: &[u8] = include_bytes!("../assets/logo.png");

/// Íconos por defecto de los componentes, los propios y el logo.
struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        if path == app::LOGO {
            return Ok(Some(Cow::Borrowed(LOGO_PNG)));
        }
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut all = gpui_kit::assets::Assets.list(path)?;
        all.extend(ExtraIcons.list(path)?);
        Ok(all)
    }
}

fn main() {
    // `notas archivo.md` abre ese archivo además de las notas de la última sesión.
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    let settings = Settings::load();
    settings.prepare_notes_dir();

    gpui_kit::application()
        .with_assets(AppAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            let mut errors = theme::load(cx);
            errors.extend(theme::apply(&settings, None, cx));
            keymap::ensure_keymap_file();
            errors.extend(keymap::load(cx));

            let window_bounds = match settings.window {
                Some(b) if b.width >= 200.0 && b.height >= 120.0 => {
                    let bounds = Bounds {
                        origin: point(px(b.x), px(b.y)),
                        size: size(px(b.width), px(b.height)),
                    };
                    if b.maximized {
                        WindowBounds::Maximized(bounds)
                    } else {
                        WindowBounds::Windowed(bounds)
                    }
                }
                _ => WindowBounds::Windowed(Bounds::centered(None, size(px(520.), px(640.)), cx)),
            };

            // Sin la barra de título de Windows: la app dibuja la suya con los
            // botones de minimizar, maximizar y cerrar.
            let options = WindowOptions {
                window_bounds: Some(window_bounds),
                window_min_size: Some(size(px(300.), px(200.))),
                ..TitleBar::window_options()
            };

            let (_, view) = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Notas::new(initial, settings, errors, window, cx))
            })
            .expect("no se pudo abrir la ventana");

            // Cerrar la ventana termina la app.
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            // "Siempre encima" necesita la ventana nativa ya creada.
            cx.defer(move |cx| {
                if let Some(window) = cx.windows().first().copied() {
                    let _ = window.update(cx, |_, window, cx| {
                        view.read(cx).apply_window_settings(window);
                    });
                }
            });
        });
}
