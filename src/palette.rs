//! Buscador de notas (`Ctrl+K`): filtra por título y contenido, sin distinguir
//! tildes ni mayúsculas, y ofrece crear una nota nueva con lo escrito.

use std::{path::PathBuf, time::SystemTime};

use gpui::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::IndexPath;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _,
    command::{Command, CommandItem, CommandState},
    h_flex,
};

use crate::notes;

/// Qué abre una entrada de la lista.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// Una nota que ya está abierta (por su id).
    Doc(usize),
    /// Un archivo de la carpeta de notas.
    File(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub target: Target,
    pub title: String,
    title_norm: String,
    body_norm: String,
    modified: Option<SystemTime>,
    /// 0 = la nota actual, 1 = abierta, 2 = en disco.
    rank: u8,
}

impl Entry {
    pub fn open(id: usize, title: String, text: &str, current: bool) -> Self {
        Self {
            target: Target::Doc(id),
            title_norm: notes::normalize(&title),
            body_norm: notes::normalize(text),
            title,
            modified: None,
            rank: if current { 0 } else { 1 },
        }
    }

    pub fn file(note: notes::NoteFile) -> Self {
        Self {
            target: Target::File(note.path),
            title: note.title,
            title_norm: note.title_norm,
            body_norm: note.body_norm,
            modified: note.modified,
            rank: 2,
        }
    }

    fn tag(&self) -> String {
        match self.rank {
            0 => "actual".into(),
            1 => "abierta".into(),
            _ => self.modified.map(notes::relative_time).unwrap_or_default(),
        }
    }
}

/// Resultados para `query`: índices en `entries` y, al final, `None` para
/// "crear nota" si lo escrito no es exactamente el título de una existente.
fn rank(entries: &[Entry], query: &str) -> Vec<Option<usize>> {
    let q = notes::normalize(query.trim());
    let mut scored: Vec<(u8, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| notes::match_score(&q, &e.title_norm, &e.body_norm).map(|s| (s, i)))
        .collect();
    // Primero lo que coincide en el título, luego lo abierto, luego lo más reciente.
    scored.sort_by(|(sa, a), (sb, b)| {
        let (ea, eb) = (&entries[*a], &entries[*b]);
        sa.cmp(sb)
            .then(ea.rank.cmp(&eb.rank))
            .then(eb.modified.cmp(&ea.modified))
    });
    let mut results: Vec<Option<usize>> = scored.into_iter().map(|(_, i)| Some(i)).collect();
    if !q.is_empty() && !entries.iter().any(|e| e.title_norm == q) {
        results.push(None);
    }
    results
}

/// Lo que se elige al confirmar.
#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    Open(Target),
    Create(String),
}

pub struct Palette {
    pub state: Entity<CommandState>,
    entries: Vec<Entry>,
    /// Resultados visibles: índice en `entries`, o `None` para "crear nota".
    results: Vec<Option<usize>>,
    query: String,
}

impl Palette {
    pub fn new<T: 'static>(entries: Vec<Entry>, window: &mut Window, cx: &mut Context<T>) -> Self {
        let state = cx.new(|cx| CommandState::new(window, cx));
        state.update(cx, |state, cx| state.focus(window, cx));
        let mut this = Self {
            state,
            entries,
            results: Vec::new(),
            query: String::new(),
        };
        this.refresh();
        this
    }

    pub fn set_query(&mut self, query: &str) {
        if self.query != query {
            self.query = query.to_string();
            self.refresh();
        }
    }

    fn refresh(&mut self) {
        self.results = rank(&self.entries, &self.query);
    }

    pub fn choice(&self, row: usize) -> Option<Choice> {
        match *self.results.get(row)? {
            Some(i) => Some(Choice::Open(self.entries[i].target.clone())),
            None => Some(Choice::Create(self.query.trim().to_string())),
        }
    }

    pub fn render(
        &self,
        on_query: impl Fn(&str, &mut Window, &mut App) + 'static,
        on_confirm: impl Fn(IndexPath, &mut Window, &mut App) + 'static,
        on_cancel: impl Fn(&mut Window, &mut App) + 'static,
        cx: &App,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let accent = theme.primary;

        let items = self.results.iter().map(|r| {
            let (icon, title, tag, create) = match r {
                Some(i) => {
                    let e = &self.entries[*i];
                    (IconName::FileText, e.title.clone(), e.tag(), false)
                }
                None => (
                    IconName::Plus,
                    format!("Crear nota «{}»", self.query.trim()),
                    "nueva".into(),
                    true,
                ),
            };
            let title: SharedString = title.into();
            CommandItem::new().label(title.clone()).child(move |_, _| {
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(
                        Icon::from(icon)
                            .small()
                            .text_color(if create { accent } else { muted }),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(title.clone()))
                    .child(div().flex_shrink_0().text_xs().text_color(muted).child(tag.clone()))
            })
        });

        Command::new(&self.state)
            .items(items)
            .filterable(false)
            .bordered(false)
            .placeholder("Buscar o crear una nota…")
            .max_h(px(340.))
            .on_query(on_query)
            .on_confirm(on_confirm)
            .on_cancel(on_cancel)
            .empty(|_, _, cx| {
                div()
                    .p_4()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No hay notas todavía. Escribe un nombre para crear una.")
            })
            .footer(move |_, _, cx| {
                h_flex()
                    .px_3()
                    .py_1p5()
                    .gap_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("↵ abrir")
                    .child("↑↓ moverse")
                    .child("Esc cerrar")
                    .child(div().flex_1())
                    .child("Ctrl+W cierra la nota")
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn file(title: &str, body: &str, age_secs: u64) -> Entry {
        let mut e = Entry::file(notes::NoteFile::from_text(
            PathBuf::from(format!("{title}.md")),
            &format!("# {title}\n{body}"),
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 - age_secs)),
        ));
        e.rank = 2;
        e
    }

    #[test]
    fn vacio_lista_abiertas_y_luego_recientes() {
        let entries = vec![
            file("vieja", "", 500),
            Entry::open(7, "Actual".into(), "# Actual", true),
            file("nueva", "", 10),
        ];
        assert_eq!(rank(&entries, ""), vec![Some(1), Some(2), Some(0)]);
    }

    #[test]
    fn titulo_antes_que_contenido_y_ofrece_crear() {
        let entries = vec![
            file("Compras", "leche y pan", 10),
            file("Pan casero", "receta", 20),
        ];
        assert_eq!(rank(&entries, "pan"), vec![Some(1), Some(0), None]);
        // Coincidencia exacta del título: no se ofrece crear.
        assert_eq!(rank(&entries, "COMPRAS"), vec![Some(0)]);
        assert_eq!(rank(&entries, "xyz"), vec![None]);
    }
}

