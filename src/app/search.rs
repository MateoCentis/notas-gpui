//! Buscador de notas (`Ctrl+K`).

use gpui::{Context, Window};

use crate::{
    keymap::SearchNotes,
    notes,
    palette::{Choice, Entry, Palette, Target},
};

use super::Notas;

impl Notas {
    pub(super) fn search_notes(
        &mut self,
        _: &SearchNotes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(super) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        self.focus_current(window, cx);
        cx.notify();
    }

    pub(super) fn confirm_palette(
        &mut self,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
}
