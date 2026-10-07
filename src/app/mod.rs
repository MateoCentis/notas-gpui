//! La ventana principal: barra de título propia, notas abiertas, editor Markdown,
//! vista previa con casillas, buscador de notas y barra de estado.

mod docs;
mod editing;
mod prefs;
mod search;
mod view;
mod window;

use std::{path::PathBuf, rc::Rc, time::Duration};

use gpui::{Context, Entity, FocusHandle, SharedString, Subscription, Task, Window};
use gpui_kit::component::input::EditorState;

use crate::{
    notes, palette::Palette, platform, settings::Settings, settings_panel::SettingsPanel,
    tasks::Block,
};

const MIN_FONT: f32 = 9.0;
const MAX_FONT: f32 = 40.0;
pub const LOGO: &str = "app/logo.png";

/// Una nota abierta. Cada una tiene su propio editor, y por lo tanto su propio deshacer.
struct Doc {
    id: usize,
    editor: Entity<EditorState>,
    /// `None` hasta que la nota nueva tiene contenido y se guarda en la carpeta de notas.
    path: Option<PathBuf>,
    /// El archivo original usaba `\r\n`; se respeta al guardar.
    crlf: bool,
    dirty: bool,
    /// Bloques de la vista previa; se recalculan solo cuando cambia el texto.
    blocks: Option<Rc<Vec<Block>>>,
    progress: (usize, usize),
    title: String,
    /// Autoguardado pendiente; reemplazarlo o soltarlo lo cancela.
    autosave_task: Option<Task<()>>,
    _subscription: Subscription,
}

pub struct Notas {
    /// Notas abiertas, de la menos a la más reciente: la última es la activa.
    docs: Vec<Doc>,
    next_id: usize,
    palette: Option<Palette>,
    settings_panel: Option<SettingsPanel>,
    focus_handle: FocusHandle,
    settings: Settings,
    /// Fuente del editor ya comprobada; `None` usa la por defecto.
    editor_font: Option<SharedString>,
    preview: bool,
    /// Mensaje temporal en la barra de estado (errores, avisos).
    status: Option<SharedString>,
    last_title: String,
    /// Un cierre falló al guardar; el siguiente intento cierra igual.
    force_close: bool,
    /// Borra `status` al vencer; reemplazarlo reinicia la cuenta.
    status_timer: Option<Task<()>>,
}

impl Notas {
    pub fn new(
        initial: Option<PathBuf>,
        settings: Settings,
        mut startup_errors: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            docs: Vec::new(),
            next_id: 0,
            palette: None,
            settings_panel: None,
            focus_handle: cx.focus_handle(),
            preview: settings.start_in_preview,
            settings,
            editor_font: None,
            status: None,
            last_title: String::new(),
            force_close: false,
            status_timer: None,
        };

        // Restaurar la sesión anterior; si no hay, la nota más reciente o una nueva.
        for path in this.settings.open_notes.clone() {
            if path.exists() {
                this.open_path(path, window, cx);
            }
        }
        if let Some(path) = initial {
            this.open_path(path, window, cx);
        }
        if this.docs.is_empty() {
            match notes::scan(&this.settings.notes_dir()).into_iter().next() {
                Some(recent) => this.open_path(recent.path, window, cx),
                None => this.new_doc(window, cx),
            }
        }
        startup_errors.extend(this.check_editor_font(cx));
        if !startup_errors.is_empty() {
            this.flash(startup_errors.join(" · "), window, cx);
        }

        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| this.should_close(window, cx))
                .unwrap_or(true)
        });

        this.focus_current(window, cx);
        this
    }

    /// Ajustes que necesitan la ventana ya creada (siempre encima).
    pub fn apply_window_settings(&self, window: &mut Window) {
        if self.settings.always_on_top {
            platform::set_always_on_top(window, true);
        }
    }

    fn focus_current(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.active() {
            Some(doc) if !self.preview => doc.editor.update(cx, |s, cx| s.focus(window, cx)),
            _ => self.focus_handle.focus(window, cx),
        }
    }

    /// Muestra un mensaje en la barra de estado durante unos segundos.
    fn flash(&mut self, msg: impl Into<SharedString>, window: &mut Window, cx: &mut Context<Self>) {
        self.status = Some(msg.into());
        self.status_timer = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(4)).await;
            let _ = this.update(cx, |this, cx| {
                this.status = None;
                cx.notify();
            });
        }));
        cx.notify();
    }
}
