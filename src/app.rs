//! La ventana principal: barra de título propia, notas abiertas, editor Markdown,
//! vista previa con casillas, buscador de notas y barra de estado.

use std::{
    ops::Range,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use gpui::{
    App, AppContext as _, ClickEvent, Context, Div, Entity, EntityId, ExternalPaths, FocusHandle,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    ParentElement as _, PathPromptOptions, Render, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, WindowBounds, div,
    img, prelude::FluentBuilder as _, px, relative,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::Selectable as _;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, StyledExt as _, TITLE_BAR_HEIGHT, TitleBar,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Editor, EditorState, InputEvent},
    text::TextView,
    v_flex,
};

use crate::{
    keymap::{
        self, CloseNote, CloseSettings, NewFile, OpenFile, OpenKeymap, OpenSettings, Quit, Save,
        SaveAs, SearchNotes, ToggleMaximize, TogglePin, TogglePreview, ToggleTask, ToggleTheme,
        ZoomIn, ZoomOut, ZoomReset,
    },
    notes,
    palette::{Choice, Entry, Palette, Target},
    platform,
    settings::{self, Settings},
    settings_panel::{Change, OnChange, SettingsPanel},
    tasks::{self, Block},
    theme,
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

    // ---------------------------------------------------------------------
    // Notas abiertas
    // ---------------------------------------------------------------------

    fn active(&self) -> Option<&Doc> {
        self.docs.last()
    }

    fn doc_ix(&self, id: usize) -> Option<usize> {
        self.docs.iter().position(|d| d.id == id)
    }

    fn doc_ix_by_editor(&self, editor: EntityId) -> Option<usize> {
        self.docs
            .iter()
            .position(|d| d.editor.entity_id() == editor)
    }

    /// Crea una nota abierta con el texto dado y la deja activa.
    fn push_doc(
        &mut self,
        path: Option<PathBuf>,
        text: String,
        crlf: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("markdown")
                .line_number(false)
                .folding(false)
                .placeholder("Escribe algo…  (Ctrl+L crea una tarea, Ctrl+K busca notas)")
        });
        editor.update(cx, |state, cx| state.set_value(text.clone(), window, cx));
        let subscription = cx.subscribe_in(
            &editor,
            window,
            |this, editor, event: &InputEvent, window, cx| {
                if let InputEvent::Change = event
                    && let Some(ix) = this.doc_ix_by_editor(editor.entity_id())
                {
                    this.on_text_changed(ix, window, cx);
                }
            },
        );

        let id = self.next_id;
        self.next_id += 1;
        self.docs.push(Doc {
            id,
            title: notes::display_title(&text, path.as_deref()),
            progress: tasks::progress(&text),
            editor,
            path,
            crlf,
            dirty: false,
            blocks: None,
            autosave_task: None,
            _subscription: subscription,
        });
        self.persist_session();
        id
    }

    /// Lleva la nota al frente (la última de la lista es la activa).
    fn activate(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.doc_ix(id) {
            let doc = self.docs.remove(ix);
            self.docs.push(doc);
            self.persist_session();
        }
        self.focus_current(window, cx);
        cx.notify();
    }

    fn new_doc(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Si la nota actual ya es una nueva vacía, no acumular otra.
        let reuse = self
            .active()
            .filter(|d| d.path.is_none() && d.editor.read(cx).value().trim().is_empty())
            .map(|d| d.id);
        match reuse {
            Some(id) => self.activate(id, window, cx),
            None => {
                self.push_doc(None, String::new(), false, window, cx);
            }
        }
        self.preview = false;
        self.focus_current(window, cx);
        cx.notify();
    }

    fn new_file(&mut self, _: &NewFile, window: &mut Window, cx: &mut Context<Self>) {
        self.new_doc(window, cx);
    }

    fn close_note(&mut self, _: &CloseNote, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.docs.len().checked_sub(1) else {
            return;
        };
        if !self.save_doc(ix, window, cx) {
            return;
        }
        self.docs.pop();
        self.persist_session();
        self.focus_current(window, cx);
        cx.notify();
    }

    /// Guarda las notas abiertas en los ajustes, para restaurarlas al reabrir.
    fn persist_session(&mut self) {
        self.settings.open_notes = self.docs.iter().filter_map(|d| d.path.clone()).collect();
        self.settings.save();
    }

    // ---------------------------------------------------------------------
    // Archivos
    // ---------------------------------------------------------------------

    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(doc) = self.docs.iter().find(|d| d.path.as_deref() == Some(&path)) {
            let id = doc.id;
            self.activate(id, window, cx);
            return;
        }
        match notes::read_file(&path) {
            Ok((text, crlf, warning)) => {
                self.push_doc(Some(path), text, crlf, window, cx);
                if let Some(w) = warning {
                    self.flash(w, window, cx);
                }
                self.focus_current(window, cx);
                cx.notify();
            }
            Err(e) => self.flash(
                format!("No se pudo abrir {}: {e}", path.display()),
                window,
                cx,
            ),
        }
    }

    /// Guarda una nota si tiene cambios. Una nota nueva recibe un archivo en la
    /// carpeta de notas, nombrado por su título; si está vacía no se crea nada.
    /// Devuelve `false` si falló la escritura.
    fn save_doc(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let notes_dir = self.settings.notes_dir();
        let doc = &mut self.docs[ix];
        doc.autosave_task = None;
        if !doc.dirty {
            return true;
        }
        let text = doc.editor.read(cx).value().to_string();
        let path = match &doc.path {
            Some(p) => p.clone(),
            None if text.trim().is_empty() => {
                doc.dirty = false;
                return true;
            }
            None => notes::unique_path(&notes_dir, &notes::display_title(&text, None)),
        };
        match notes::write_file(&path, &text, doc.crlf) {
            Ok(()) => {
                let is_new = doc.path.is_none();
                let is_settings = path == Settings::path();
                doc.path = Some(path);
                doc.dirty = false;
                if is_new {
                    self.persist_session();
                }
                if is_settings {
                    self.reload_settings(window, cx);
                }
                cx.notify();
                true
            }
            Err(e) => {
                self.flash(format!("Error al guardar: {e}"), window, cx);
                false
            }
        }
    }

    fn save_all(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        // Sin cortocircuito: si una falla, se intenta guardar el resto igual.
        let mut ok = true;
        for ix in 0..self.docs.len() {
            ok &= self.save_doc(ix, window, cx);
        }
        ok
    }

    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.docs.len().checked_sub(1) else {
            return;
        };
        self.docs[ix].dirty = true; // Guardar explícito siempre escribe.
        if self.save_doc(ix, window, cx) {
            self.flash("Guardado", window, cx);
        }
    }

    fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.active() else { return };
        let id = doc.id;
        let dir = doc
            .path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| self.settings.notes_dir());
        let name = match &doc.path {
            Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()),
            None => Some(format!("{}.md", notes::slug(&doc.title))),
        };
        let rx = cx.prompt_for_new_path(&dir, name.as_deref());
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(ix) = this.doc_ix(id) else { return };
                let doc = &this.docs[ix];
                let text = doc.editor.read(cx).value().to_string();
                match notes::write_file(&path, &text, doc.crlf) {
                    Ok(()) => {
                        let doc = &mut this.docs[ix];
                        doc.path = Some(path);
                        doc.dirty = false;
                        this.persist_session();
                        this.flash("Guardado", window, cx);
                    }
                    Err(e) => this.flash(format!("Error al guardar: {e}"), window, cx),
                }
            });
        })
        .detach();
    }

    fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                for path in paths {
                    this.open_path(path, window, cx);
                }
            });
        })
        .detach();
    }

    fn open_keymap(&mut self, _: &OpenKeymap, window: &mut Window, cx: &mut Context<Self>) {
        keymap::ensure_keymap_file();
        self.preview = false;
        self.open_path(keymap::keymap_path(), window, cx);
        self.flash(
            "Los cambios de atajos se aplican al reiniciar la app",
            window,
            cx,
        );
    }

    // ---------------------------------------------------------------------
    // Ajustes (Ctrl+,)
    // ---------------------------------------------------------------------

    fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
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
        self.settings_panel = Some(SettingsPanel::new(
            &self.settings,
            Self::on_settings_change,
            window,
            cx,
        ));
        cx.notify();
    }

    fn close_settings(&mut self, _: &CloseSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_panel = None;
        self.focus_current(window, cx);
        cx.notify();
    }

    fn on_settings_change(&mut self, change: Change, window: &mut Window, cx: &mut Context<Self>) {
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

    /// Aplica tema y fuentes de los ajustes actuales; avisa de lo que no se pudo aplicar.
    fn apply_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut errors = theme::apply(&self.settings, Some(window), cx);
        errors.extend(self.check_editor_font(cx));
        if !errors.is_empty() {
            self.flash(errors.join(" · "), window, cx);
        }
        cx.notify();
    }

    /// Comprueba que la fuente del editor esté instalada antes de usarla.
    fn check_editor_font(&mut self, cx: &App) -> Option<String> {
        self.editor_font = None;
        let family = self.settings.font_family.as_deref()?;
        self.editor_font = theme::installed_font(family, cx);
        if self.editor_font.is_none() {
            return Some(format!("La fuente «{family}» no está instalada"));
        }
        None
    }

    /// `settings.json` se guardó desde el editor: aplicar lo que cambió.
    fn reload_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    // ---------------------------------------------------------------------
    // Buscador (Ctrl+K)
    // ---------------------------------------------------------------------

    fn search_notes(&mut self, _: &SearchNotes, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            self.close_palette(window, cx);
            return;
        }
        self.settings_panel = None;
        let current = self.active().map(|d| d.id);
        let mut entries: Vec<Entry> = self
            .docs
            .iter()
            .rev()
            .map(|d| {
                let text = d.editor.read(cx).value();
                Entry::open(d.id, d.title.clone(), &text, Some(d.id) == current)
            })
            .collect();
        entries.extend(
            notes::scan(&self.settings.notes_dir())
                .into_iter()
                .filter(|f| !self.docs.iter().any(|d| d.path.as_ref() == Some(&f.path)))
                .map(Entry::file),
        );
        self.palette = Some(Palette::new(entries, window, cx));
        cx.notify();
    }

    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        self.focus_current(window, cx);
        cx.notify();
    }

    fn confirm_palette(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(choice) = self.palette.as_ref().and_then(|p| p.choice(row)) else {
            return;
        };
        self.palette = None;
        match choice {
            Choice::Open(Target::Doc(id)) => self.activate(id, window, cx),
            Choice::Open(Target::File(path)) => self.open_path(path, window, cx),
            Choice::Create(name) => {
                self.preview = false;
                let text = format!("# {name}\n\n");
                let id = self.push_doc(None, text.clone(), false, window, cx);
                if let Some(ix) = self.doc_ix(id) {
                    let editor = self.docs[ix].editor.clone();
                    editor.update(cx, |s, cx| s.set_selected_range(text.len()..text.len(), cx));
                    self.on_text_changed(ix, window, cx);
                }
                self.focus_current(window, cx);
            }
        }
        cx.notify();
    }

    // ---------------------------------------------------------------------
    // Edición y tareas
    // ---------------------------------------------------------------------

    fn on_text_changed(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let delay = Duration::from_millis(self.settings.autosave_delay_ms);
        let autosave = self.settings.autosave;
        let doc = &mut self.docs[ix];
        let text = doc.editor.read(cx).value();
        doc.dirty = true;
        doc.blocks = None;
        doc.progress = tasks::progress(&text);
        doc.title = notes::display_title(&text, doc.path.as_deref());
        if autosave {
            let id = doc.id;
            // Reemplazar la tarea anterior la cancela: así se espera a que dejes de escribir.
            doc.autosave_task = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(delay).await;
                let _ = this.update_in(cx, |this, window, cx| {
                    if let Some(ix) = this.doc_ix(id) {
                        this.save_doc(ix, window, cx);
                    }
                });
            }));
        }
        cx.notify();
    }

    fn blocks(&mut self, cx: &mut Context<Self>) -> Rc<Vec<Block>> {
        let Some(doc) = self.docs.last_mut() else {
            return Rc::default();
        };
        doc.blocks
            .get_or_insert_with(|| Rc::new(tasks::split_blocks(&doc.editor.read(cx).value())))
            .clone()
    }

    /// Reemplaza `range` por `text` en la nota activa, conservando el deshacer.
    fn apply_edit(
        &mut self,
        range: Range<usize>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.docs.len().checked_sub(1) else {
            return;
        };
        let text = text.to_string();
        self.docs[ix].editor.update(cx, |state, cx| {
            let cursor = state.cursor();
            let inserted = text.len();
            state.set_selected_range(range.clone(), cx);
            state.replace(text, window, cx);
            // Devolver el cursor a donde estaba, corrido si la edición quedó antes.
            let cursor = if cursor >= range.end {
                cursor - range.len() + inserted
            } else {
                cursor
            };
            state.set_selected_range(cursor..cursor, cx);
        });
        self.on_text_changed(ix, window, cx);
    }

    fn toggle_task(&mut self, _: &ToggleTask, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview {
            return;
        }
        let Some(doc) = self.active() else { return };
        let (text, cursor) = {
            let state = doc.editor.read(cx);
            (state.value().to_string(), state.cursor())
        };
        let edit = tasks::toggle_task_edit(&text, cursor);
        self.apply_edit(edit.range, edit.text, window, cx);
    }

    /// Clic en una casilla de la vista previa.
    fn set_mark(
        &mut self,
        mark: Range<usize>,
        checked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.active() else { return };
        // Comprobar que el texto no cambió desde que se dibujó la casilla.
        let current = doc.editor.read(cx).value();
        if current
            .get(mark.clone())
            .is_none_or(|c| c != " " && c != "x" && c != "X")
        {
            return;
        }
        self.apply_edit(mark, if checked { "x" } else { " " }, window, cx);
    }

    // ---------------------------------------------------------------------
    // Ventana y vista
    // ---------------------------------------------------------------------

    fn focus_current(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.active() {
            Some(doc) if !self.preview => doc.editor.update(cx, |s, cx| s.focus(window, cx)),
            _ => self.focus_handle.focus(window, cx),
        }
    }

    fn toggle_preview(&mut self, _: &TogglePreview, window: &mut Window, cx: &mut Context<Self>) {
        self.preview = !self.preview;
        self.focus_current(window, cx);
        cx.notify();
    }

    fn toggle_pin(&mut self, _: &TogglePin, window: &mut Window, cx: &mut Context<Self>) {
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

    fn toggle_maximize(&mut self, _: &ToggleMaximize, window: &mut Window, _: &mut Context<Self>) {
        window.zoom_window();
    }

    fn toggle_theme(&mut self, _: &ToggleTheme, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.dark = !self.settings.dark;
        self.settings.save();
        self.apply_appearance(window, cx);
    }

    fn set_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        self.settings.font_size = size.clamp(MIN_FONT, MAX_FONT);
        self.settings.save();
        cx.notify();
    }

    fn zoom_in(&mut self, _: &ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        self.set_font_size(self.settings.font_size + 1.0, cx);
    }

    fn zoom_out(&mut self, _: &ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.set_font_size(self.settings.font_size - 1.0, cx);
    }

    fn zoom_reset(&mut self, _: &ZoomReset, _: &mut Window, cx: &mut Context<Self>) {
        self.set_font_size(Settings::default().font_size, cx);
    }

    fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if self.should_close(window, cx) {
            window.remove_window();
        }
    }

    /// Se llama al cerrar la ventana: guarda todo y recuerda sesión y posición.
    fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
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

    // ---------------------------------------------------------------------
    // Render
    // ---------------------------------------------------------------------

    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (title, dirty) = match self.active() {
            Some(d) => (d.title.clone(), d.dirty),
            None => ("Notas".into(), false),
        };
        let open = self.docs.len();

        let icon_button = |id: &'static str, icon: IconName, tooltip: &'static str| {
            Button::new(id).ghost().xsmall().icon(icon).tooltip(tooltip)
        };

        TitleBar::new()
            .bg(theme.title_bar)
            .border_color(theme.title_bar_border)
            .child(
                h_flex()
                    .min_w_0()
                    .gap_2()
                    .items_center()
                    .child(img(LOGO).size(px(16.)).flex_shrink_0())
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .font_medium()
                            .text_color(theme.foreground)
                            .child(title),
                    )
                    .when(dirty, |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .size(px(6.))
                                .rounded_full()
                                .bg(theme.primary),
                        )
                    })
                    .when(open > 1, |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .px_1p5()
                                .rounded_md()
                                .bg(theme.secondary)
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("{open} abiertas")),
                        )
                    }),
            )
            .child(
                h_flex()
                    .id("title-actions")
                    // Tapa la zona de arrastre de la barra: sin esto Windows trata el
                    // clic como "mover ventana" y los botones no reciben el clic.
                    .occlude()
                    .flex_shrink_0()
                    .gap_0p5()
                    .pr_1()
                    .child(
                        icon_button("search", IconName::Search, "Buscar notas (Ctrl+K)").on_click(
                            cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.search_notes(&SearchNotes, window, cx)
                            }),
                        ),
                    )
                    .child(
                        icon_button(
                            "preview",
                            if self.preview {
                                IconName::Pencil
                            } else {
                                IconName::Eye
                            },
                            if self.preview {
                                "Editar (Ctrl+E)"
                            } else {
                                "Vista (Ctrl+E)"
                            },
                        )
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, window, cx| {
                                this.toggle_preview(&TogglePreview, window, cx)
                            },
                        )),
                    )
                    .child(
                        icon_button(
                            "pin",
                            if self.settings.always_on_top {
                                IconName::PinOff
                            } else {
                                IconName::Pin
                            },
                            "Siempre encima (Ctrl+Shift+T)",
                        )
                        .selected(self.settings.always_on_top)
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, window, cx| {
                                this.toggle_pin(&TogglePin, window, cx)
                            },
                        )),
                    )
                    .child(
                        icon_button("settings", IconName::Settings, "Ajustes (Ctrl+,)")
                            .selected(self.settings_panel.is_some())
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.open_settings(&OpenSettings, window, cx)
                            })),
                    )
                    .child(
                        icon_button(
                            "theme",
                            if self.settings.dark {
                                IconName::Sun
                            } else {
                                IconName::Moon
                            },
                            "Tema claro/oscuro (Ctrl+Shift+D)",
                        )
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, window, cx| {
                                this.toggle_theme(&ToggleTheme, window, cx)
                            },
                        )),
                    ),
            )
    }

    fn render_preview(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let blocks = self.blocks(cx);
        let id = self.active().map_or(0, |d| d.id);
        let muted = cx.theme().muted_foreground;
        let entity = cx.entity().downgrade();

        let content = if blocks.is_empty() {
            v_flex().child(
                div()
                    .text_color(muted)
                    .child("Nota vacía. Pulsa Ctrl+E para escribir."),
            )
        } else {
            v_flex()
                .gap_1()
                .children(blocks.iter().enumerate().map(|(i, block)| {
                    match block {
                        Block::Markdown(md) => div()
                            .py_1()
                            .child(TextView::markdown(("md", i), md.clone()).selectable(true))
                            .into_any_element(),
                        Block::Task(task) => {
                            let mark = task.mark.clone();
                            let entity = entity.clone();
                            h_flex()
                                .items_start()
                                .gap_2()
                                .pl(px(task.indent as f32 * 8.0))
                                .child(div().pt(px(3.)).child(
                                    Checkbox::new(("task", i)).checked(task.checked).on_click(
                                        move |checked, window, cx| {
                                            let mark = mark.clone();
                                            let _ = entity.update(cx, |this, cx| {
                                                this.set_mark(mark, *checked, window, cx)
                                            });
                                        },
                                    ),
                                ))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .when(task.checked, |d| d.text_color(muted).line_through())
                                        .child(TextView::markdown(
                                            ("task-text", i),
                                            task.text.clone(),
                                        )),
                                )
                                .into_any_element()
                        }
                    }
                }))
        };

        div()
            .id(("preview", id))
            .size_full()
            .overflow_y_scroll()
            .px_5()
            .py_3()
            .child(content)
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let hint = |keys: &'static str, text: &'static str| {
            h_flex()
                .gap_3()
                .items_center()
                .child(
                    div()
                        .w(px(64.))
                        .px_1p5()
                        .py_0p5()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.secondary)
                        .text_xs()
                        .text_center()
                        .child(keys),
                )
                .child(div().text_color(muted).child(text))
        };
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .child(img(LOGO).size(px(56.)).opacity(0.9))
            .child(div().text_color(muted).child("No hay ninguna nota abierta"))
            .child(
                v_flex()
                    .gap_2()
                    .text_sm()
                    .child(hint("Ctrl N", "Nueva nota"))
                    .child(hint("Ctrl K", "Buscar notas")),
            )
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let location = match self.active() {
            Some(Doc { path: Some(p), .. }) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            Some(_) => "Sin guardar".into(),
            None => String::new(),
        };
        let (done, total) = self.active().map_or((0, 0), |d| d.progress);

        h_flex()
            .h(px(26.))
            .px_3()
            .gap_3()
            .border_t_1()
            .border_color(theme.title_bar_border)
            .bg(theme.title_bar)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(div().truncate().child(location))
            .when_some(self.status.clone(), |this, status| {
                this.child(div().truncate().text_color(theme.primary).child(status))
            })
            .child(div().flex_1())
            .when(total > 0, |this| {
                let ratio = done as f32 / total as f32;
                this.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .w(px(56.))
                                .h(px(4.))
                                .rounded_full()
                                .bg(theme.secondary)
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(ratio))
                                        .rounded_full()
                                        .bg(theme.primary),
                                ),
                        )
                        .child(format!("{done}/{total} tareas")),
                )
            })
    }

    fn render_palette(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let palette = self.palette.as_ref()?;
        let weak = cx.entity().downgrade();
        let (q, c, x) = (weak.clone(), weak.clone(), weak.clone());

        let command = palette.render(
            move |query, _, cx| {
                let _ = q.update(cx, |this, cx| {
                    if let Some(p) = this.palette.as_mut() {
                        p.set_query(query);
                    }
                    cx.notify();
                });
            },
            move |ix, window, cx| {
                let _ = c.update(cx, |this, cx| this.confirm_palette(ix.row, window, cx));
            },
            move |window, cx| {
                let _ = x.update(cx, |this, cx| this.close_palette(window, cx));
            },
            cx,
        );

        Some(Self::render_modal(
            "palette-backdrop",
            cx.listener(|this, _, window, cx| this.close_palette(window, cx)),
            cx,
            Self::modal_panel("palette", cx)
                // Escape con la búsqueda vacía cierra; con texto, lo borra la paleta.
                .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    if ev.keystroke.key != "escape" || ev.keystroke.modifiers.modified() {
                        return;
                    }
                    let empty = this
                        .palette
                        .as_ref()
                        .is_none_or(|p| p.state.read(cx).query(cx).is_empty());
                    if empty {
                        cx.stop_propagation();
                        this.close_palette(window, cx);
                    }
                }))
                .child(command),
        ))
    }

    fn render_settings(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let panel = self.settings_panel.as_ref()?;
        let weak = cx.entity().downgrade();
        let on_change: OnChange = Rc::new(move |change, window, cx| {
            let _ = weak.update(cx, |this, cx| this.on_settings_change(change, window, cx));
        });

        Some(Self::render_modal(
            "settings-backdrop",
            cx.listener(|this, _, window, cx| this.close_settings(&CloseSettings, window, cx)),
            cx,
            Self::modal_panel("settings", cx)
                .key_context(keymap::SETTINGS_CONTEXT)
                .track_focus(&panel.focus_handle)
                .text_color(cx.theme().popover_foreground)
                // El tamaño de letra del editor no se aplica al panel.
                .text_size(px(14.))
                .child(panel.render(&self.settings, &on_change, cx)),
        ))
    }

    /// Caja de un panel flotante (buscador o ajustes).
    fn modal_panel(id: &'static str, cx: &App) -> Stateful<Div> {
        let theme = cx.theme();
        div()
            .id(id)
            .w_full()
            .max_w(px(460.))
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_lg()
            .overflow_hidden()
            // Los clics dentro del panel no deben cerrarlo.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }

    /// Fondo oscurecido bajo un panel flotante; un clic fuera del panel llama a `on_close`.
    fn render_modal(
        id: &'static str,
        on_close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &App,
        panel: impl IntoElement,
    ) -> impl IntoElement {
        div()
            .id(id)
            .absolute()
            // Debajo de la barra de título, para poder mover o cerrar la ventana.
            .top(TITLE_BAR_HEIGHT)
            .left_0()
            .right_0()
            .bottom_0()
            .occlude()
            .bg(cx.theme().overlay)
            .flex()
            .flex_col()
            .items_center()
            .pt(px(20.))
            .px_4()
            .on_mouse_down(MouseButton::Left, on_close)
            .child(panel)
    }
}

impl Render for Notas {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match self.active() {
            Some(d) => format!("{}{} — Notas", if d.dirty { "• " } else { "" }, d.title),
            None => "Notas".into(),
        };
        if title != self.last_title {
            window.set_window_title(&title);
            self.last_title = title;
        }

        let font_size = px(self.settings.font_size);
        let font_family = self.editor_font.clone();

        let body = match self.active() {
            None => self.render_empty(cx).into_any_element(),
            Some(_) if self.preview => self.render_preview(cx).into_any_element(),
            Some(doc) => div()
                .size_full()
                .pl_5()
                .pr_1()
                .pt_3()
                .pb_1()
                .child(
                    Editor::new(&doc.editor)
                        .appearance(false)
                        .h_full()
                        .text_size(font_size)
                        .when_some(font_family, |e, f| e.font_family(f)),
                )
                .into_any_element(),
        };

        v_flex()
            .relative()
            .key_context(keymap::CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::new_file))
            .on_action(cx.listener(Self::open_file))
            .on_action(cx.listener(Self::search_notes))
            .on_action(cx.listener(Self::close_note))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::toggle_preview))
            .on_action(cx.listener(Self::toggle_pin))
            .on_action(cx.listener(Self::toggle_maximize))
            .on_action(cx.listener(Self::toggle_task))
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
            .on_action(cx.listener(Self::zoom_reset))
            .on_action(cx.listener(Self::open_keymap))
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(Self::close_settings))
            .on_action(cx.listener(Self::quit))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                for path in paths.paths() {
                    this.open_path(path.clone(), window, cx);
                }
            }))
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_title_bar(cx))
            .child(div().flex_1().min_h_0().text_size(font_size).child(body))
            .child(self.render_status_bar(cx))
            .children(self.render_palette(cx))
            .children(self.render_settings(cx))
    }
}
