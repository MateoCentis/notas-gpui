//! Notas abiertas y archivos: crear, activar, abrir, guardar y cerrar.

use std::path::{Path, PathBuf};

use gpui::{AppContext as _, Context, EntityId, PathPromptOptions, Window};
use gpui_kit::component::input::{EditorState, InputEvent};

use crate::{
    keymap::{self, CloseNote, DeleteNote, NewFile, OpenFile, OpenKeymap, Save, SaveAs},
    notes, platform,
    settings::Settings,
    tasks,
};

use super::{Doc, Notas};

impl Notas {
    pub(super) fn active(&self) -> Option<&Doc> {
        self.docs.last()
    }

    pub(super) fn doc_ix(&self, id: usize) -> Option<usize> {
        self.docs.iter().position(|d| d.id == id)
    }

    fn doc_ix_by_editor(&self, editor: EntityId) -> Option<usize> {
        self.docs
            .iter()
            .position(|d| d.editor.entity_id() == editor)
    }

    /// Crea una nota abierta con el texto dado y la deja activa.
    pub(super) fn push_doc(
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
                .placeholder(placeholder(cx))
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
    pub(super) fn activate(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.doc_ix(id) {
            let doc = self.docs.remove(ix);
            self.docs.push(doc);
            self.persist_session();
        }
        self.focus_current(window, cx);
        cx.notify();
    }

    pub(super) fn new_doc(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn new_file(&mut self, _: &NewFile, window: &mut Window, cx: &mut Context<Self>) {
        self.new_doc(window, cx);
    }

    pub(super) fn close_note(
        &mut self,
        _: &CloseNote,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    /// Borra la nota activa: el archivo va a la papelera y la nota se cierra.
    pub(super) fn delete_note(
        &mut self,
        _: &DeleteNote,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.docs.pop() else { return };
        if let Some(path) = &doc.path
            && let Err(e) = platform::move_to_trash(path)
        {
            self.docs.push(doc);
            self.flash(format!("No se pudo borrar la nota: {e}"), window, cx);
            return;
        }
        let msg = match doc.path {
            Some(_) => format!("«{}» se movió a la papelera", doc.title),
            None => "Nota descartada".to_string(),
        };
        self.persist_session();
        self.focus_current(window, cx);
        self.flash(msg, window, cx);
    }

    /// Guarda las notas abiertas en los ajustes, para restaurarlas al reabrir.
    pub(super) fn persist_session(&mut self) {
        self.settings.open_notes = self.docs.iter().filter_map(|d| d.path.clone()).collect();
        self.settings.save();
    }

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
    pub(super) fn save_doc(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
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
                let is_keymap = path == keymap::keymap_path();
                doc.path = Some(path);
                doc.dirty = false;
                if is_new {
                    self.persist_session();
                }
                if is_settings {
                    self.reload_settings(window, cx);
                }
                if is_keymap {
                    let errors = keymap::load(cx);
                    let msg = if errors.is_empty() {
                        "Atajos aplicados".to_string()
                    } else {
                        errors.join(" · ")
                    };
                    self.flash(msg, window, cx);
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

    pub(super) fn save_all(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        // Sin cortocircuito: si una falla, se intenta guardar el resto igual.
        let mut ok = true;
        for ix in 0..self.docs.len() {
            ok &= self.save_doc(ix, window, cx);
        }
        ok
    }

    pub(super) fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.docs.len().checked_sub(1) else {
            return;
        };
        self.docs[ix].dirty = true; // Guardar explícito siempre escribe.
        if self.save_doc(ix, window, cx) {
            self.flash("Guardado", window, cx);
        }
    }

    pub(super) fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn open_keymap(
        &mut self,
        _: &OpenKeymap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        keymap::ensure_keymap_file();
        self.preview = false;
        self.open_path(keymap::keymap_path(), window, cx);
        self.flash("Los cambios se aplican al guardar el archivo", window, cx);
    }
}

/// Texto de una nota vacía, con los atajos actuales.
fn placeholder(cx: &gpui::App) -> String {
    let mut tips = Vec::new();
    if let Some(keys) = keymap::label("ToggleTask", cx) {
        tips.push(format!("{keys} crea una tarea"));
    }
    if let Some(keys) = keymap::label("SearchNotes", cx) {
        tips.push(format!("{keys} busca notas"));
    }
    if tips.is_empty() {
        return "Escribe algo…".into();
    }
    format!("Escribe algo…  ({})", tips.join(", "))
}
