//! Panel de ajustes (`Ctrl+,`), apariencia, atajos y recarga de `settings.json`.

use gpui::{App, Context, Keystroke, Window};

use crate::{
    keymap::{self, CloseSettings, OpenSettings, TogglePin},
    platform,
    settings::Settings,
    settings_panel::{Change, SettingsPanel},
    theme,
};

use super::{MAX_FONT, MIN_FONT, Notas};

impl Notas {
    pub(super) fn open_settings(
        &mut self,
        _: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_panel.is_some() {
            self.close_settings(&CloseSettings, window, cx);
            return;
        }
        self.palette = None;
        // Releer temas y fuentes: puede haber archivos o fuentes nuevas.
        let errors = theme::load(cx);
        if !errors.is_empty() {
            self.flash(errors.join(" · "), window, cx);
        }
        let mut panel = SettingsPanel::new(&self.settings, Self::on_settings_change, window, cx);
        // Los interceptores ven la tecla antes que los atajos: así se puede grabar
        // cualquier combinación, incluso una que ya esté asignada.
        let weak = cx.entity().downgrade();
        panel.key_recorder = Some(cx.intercept_keystrokes(move |ev, window, cx| {
            let _ = weak.update(cx, |this, cx| {
                if this.record_shortcut(&ev.keystroke, window, cx) {
                    cx.stop_propagation();
                }
            });
        }));
        self.settings_panel = Some(panel);
        cx.notify();
    }

    pub(super) fn close_settings(
        &mut self,
        _: &CloseSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_panel = None;
        self.focus_current(window, cx);
        cx.notify();
    }

    pub(super) fn on_settings_change(
        &mut self,
        change: Change,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match change {
            Change::Theme(name) => {
                self.settings.theme = name.to_string();
                self.apply_appearance(window, cx);
            }
            Change::Dark(dark) => {
                self.settings.dark = dark;
                self.apply_appearance(window, cx);
            }
            Change::EditorFont(family) => {
                self.settings.font_family = family.map(|f| f.to_string());
                self.apply_appearance(window, cx);
            }
            Change::UiFont(family) => {
                self.settings.ui_font_family = family.map(|f| f.to_string());
                self.apply_appearance(window, cx);
            }
            Change::FontSize(size) => self.set_font_size(size, cx),
            Change::AlwaysOnTop(on) => {
                if on != self.settings.always_on_top {
                    self.toggle_pin(&TogglePin, window, cx);
                }
            }
            Change::Autosave(on) => self.settings.autosave = on,
            Change::StartInPreview(on) => self.settings.start_in_preview = on,
            Change::Record(action) => {
                if let Some(panel) = self.settings_panel.as_mut() {
                    panel.recording = (panel.recording != Some(action)).then_some(action);
                }
            }
            Change::ResetShortcuts => {
                let errors = keymap::reset(cx);
                let msg = if errors.is_empty() {
                    "Atajos restablecidos".to_string()
                } else {
                    errors.join(" · ")
                };
                self.flash(msg, window, cx);
            }
            Change::EditFile => {
                self.settings_panel = None;
                self.settings.save();
                self.preview = false;
                self.open_path(Settings::path(), window, cx);
                self.flash("Los cambios se aplican al guardar el archivo", window, cx);
            }
            Change::OpenThemesFolder => {
                let dir = theme::themes_dir();
                let _ = std::fs::create_dir_all(&dir);
                cx.open_with_system(&dir);
            }
            Change::Close => self.close_settings(&CloseSettings, window, cx),
        }
        self.settings.save();
        cx.notify();
    }

    /// Si se está grabando un atajo, `keystroke` lo reemplaza. Devuelve `true` si la
    /// tecla se usó y no debe llegar a nadie más.
    fn record_shortcut(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(action) = self.settings_panel.as_ref().and_then(|p| p.recording) else {
            return false;
        };
        let key = keystroke.key.as_str();
        let m = keystroke.modifiers;
        // Esperar a la tecla que acompaña a los modificadores.
        if matches!(key, "control" | "alt" | "shift" | "platform" | "function") {
            return true;
        }
        let plain = !m.control && !m.alt && !m.platform;
        let function_key = key.len() > 1 && key.starts_with('f') && key[1..].parse::<u8>().is_ok();
        let result = match key {
            "escape" if !m.modified() => Ok(None),
            "delete" | "backspace" if !m.modified() => keymap::set_shortcut(action, None, cx),
            // Sin Ctrl ni Alt, una letra escribiría texto en vez de ejecutar el atajo.
            _ if plain && !function_key => {
                self.flash("Usa una combinación con Ctrl o Alt", window, cx);
                return true;
            }
            _ => keymap::set_shortcut(action, Some(&keystroke.unparse()), cx),
        };
        if let Some(panel) = self.settings_panel.as_mut() {
            panel.recording = None;
        }
        match result {
            Ok(Some(notice)) => self.flash(notice, window, cx),
            Ok(None) => {}
            Err(e) => self.flash(e, window, cx),
        }
        cx.notify();
        true
    }

    /// Aplica tema y fuentes de los ajustes actuales; avisa de lo que no se pudo aplicar.
    pub(super) fn apply_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut errors = theme::apply(&self.settings, Some(window), cx);
        errors.extend(self.check_editor_font(cx));
        if !errors.is_empty() {
            self.flash(errors.join(" · "), window, cx);
        }
        cx.notify();
    }

    /// Comprueba que la fuente del editor esté instalada antes de usarla.
    pub(super) fn check_editor_font(&mut self, cx: &App) -> Option<String> {
        self.editor_font = None;
        let family = self.settings.font_family.as_deref()?;
        self.editor_font = theme::installed_font(family, cx);
        if self.editor_font.is_none() {
            return Some(format!("La fuente «{family}» no está instalada"));
        }
        None
    }

    /// `settings.json` se guardó desde el editor: aplicar lo que cambió.
    pub(super) fn reload_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut new = match Settings::read() {
            Ok(Some(new)) => new,
            Ok(None) => return,
            Err(e) => {
                self.flash(e, window, cx);
                return;
            }
        };
        // Estado de la sesión, no preferencias: se conserva el actual.
        new.open_notes = std::mem::take(&mut self.settings.open_notes);
        new.window = self.settings.window;
        new.font_size = new.font_size.clamp(MIN_FONT, MAX_FONT);
        if new.always_on_top != self.settings.always_on_top {
            platform::set_always_on_top(window, new.always_on_top);
        }
        self.settings = new;
        self.apply_appearance(window, cx);
        if self.status.is_none() {
            self.flash("Ajustes aplicados", window, cx);
        }
    }
}
