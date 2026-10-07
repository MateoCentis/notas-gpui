//! Ventana y vista: modo vista, siempre encima, tema, zoom y cierre.

use gpui::{Context, Window, WindowBounds};

use crate::{
    keymap::{
        Quit, ToggleMaximize, TogglePin, TogglePreview, ToggleTheme, ZoomIn, ZoomOut, ZoomReset,
    },
    platform,
    settings::{self, Settings},
};

use super::{MAX_FONT, MIN_FONT, Notas};

impl Notas {
    pub(super) fn toggle_preview(
        &mut self,
        _: &TogglePreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview = !self.preview;
        self.focus_current(window, cx);
        cx.notify();
    }

    pub(super) fn toggle_pin(
        &mut self,
        _: &TogglePin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.always_on_top = !self.settings.always_on_top;
        platform::set_always_on_top(window, self.settings.always_on_top);
        self.settings.save();
        let msg = if self.settings.always_on_top {
            "Siempre encima: activado"
        } else {
            "Siempre encima: desactivado"
        };
        self.flash(msg, window, cx);
    }

    pub(super) fn toggle_maximize(
        &mut self,
        _: &ToggleMaximize,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.zoom_window();
    }

    pub(super) fn toggle_theme(
        &mut self,
        _: &ToggleTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.dark = !self.settings.dark;
        self.settings.save();
        self.apply_appearance(window, cx);
    }

    pub(super) fn set_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        self.settings.font_size = size.clamp(MIN_FONT, MAX_FONT);
        self.settings.save();
        cx.notify();
    }

    pub(super) fn zoom_in(&mut self, _: &ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        self.set_font_size(self.settings.font_size + 1.0, cx);
    }

    pub(super) fn zoom_out(&mut self, _: &ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.set_font_size(self.settings.font_size - 1.0, cx);
    }

    pub(super) fn zoom_reset(&mut self, _: &ZoomReset, _: &mut Window, cx: &mut Context<Self>) {
        self.set_font_size(Settings::default().font_size, cx);
    }

    pub(super) fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if self.should_close(window, cx) {
            window.remove_window();
        }
    }

    /// Se llama al cerrar la ventana: guarda todo y recuerda sesión y posición.
    pub(super) fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.save_all(window, cx) && !self.force_close {
            self.force_close = true;
            self.flash(
                "No se pudo guardar. Vuelve a cerrar para salir sin guardar.",
                window,
                cx,
            );
            return false;
        }
        // `window_bounds` da el tamaño "restaurado" aunque esté maximizada, en el
        // mismo sistema de coordenadas que acepta `WindowOptions` al reabrir.
        let (b, maximized) = match window.window_bounds() {
            WindowBounds::Windowed(b) => (b, false),
            WindowBounds::Maximized(b) => (b, true),
            WindowBounds::Fullscreen(b) => (b, false),
        };
        self.settings.window = Some(settings::SavedBounds {
            x: f32::from(b.origin.x),
            y: f32::from(b.origin.y),
            width: f32::from(b.size.width),
            height: f32::from(b.size.height),
            maximized,
        });
        self.persist_session();
        true
    }
}
