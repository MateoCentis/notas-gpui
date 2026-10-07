//! Edición del texto y tareas: autoguardado, vista previa y casillas.

use std::{ops::Range, rc::Rc, time::Duration};

use gpui::{Context, Window};

use crate::{
    keymap::ToggleTask,
    notes,
    tasks::{self, Block},
};

use super::Notas;

impl Notas {
    pub(super) fn on_text_changed(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(super) fn blocks(&mut self, cx: &mut Context<Self>) -> Rc<Vec<Block>> {
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

    pub(super) fn toggle_task(
        &mut self,
        _: &ToggleTask,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    pub(super) fn set_mark(
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
}
