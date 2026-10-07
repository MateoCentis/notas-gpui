//! Lógica pura sobre el texto Markdown: detectar tareas (`- [ ] algo`),
//! marcarlas/desmarcarlas y partir el documento en bloques para la vista previa.

use std::ops::Range;

/// Una línea de tarea (`- [ ] texto`, `* [x] texto`, `1. [ ] texto`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// Nivel de sangría (en columnas; un tab cuenta como 4).
    pub indent: usize,
    pub checked: bool,
    /// Rango en bytes del carácter entre corchetes (` ` o `x`), relativo al documento.
    pub mark: Range<usize>,
    /// Texto de la tarea tras la casilla.
    pub text: String,
}

/// Un trozo del documento tal como se muestra en la vista previa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// Markdown corriente que se renderiza tal cual.
    Markdown(String),
    /// Una tarea, que se dibuja como casilla clicable.
    Task(Task),
}

/// Si `line` es una tarea, devuelve `(sangría, marcada, offset de la marca, texto)`.
/// El offset de la marca es relativo al comienzo de la línea.
fn parse_task_line(line: &str) -> Option<(usize, bool, usize, &str)> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let lead = &line[..line.len() - trimmed.len()];
    let indent = lead.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum();

    // Viñeta: `-`, `*`, `+` o `N.` / `N)`
    let after_bullet = if let Some(rest) = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))
    {
        rest
    } else {
        let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || digits > 9 {
            return None;
        }
        let rest = &trimmed[digits..];
        rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") "))?
    };

    let after_spaces = after_bullet.trim_start_matches(' ');
    let bytes = after_spaces.as_bytes();
    if bytes.len() < 3 || bytes[0] != b'[' || bytes[2] != b']' {
        return None;
    }
    let checked = match bytes[1] {
        b' ' => false,
        b'x' | b'X' => true,
        _ => return None,
    };
    let rest = &after_spaces[3..];
    // Tras `]` debe venir un espacio o el fin de la línea.
    if !(rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')) {
        return None;
    }
    let mark_offset = line.len() - after_spaces.len() + 1;
    Some((indent, checked, mark_offset, rest.trim()))
}

/// Comienzo de un bloque de código cercado (``` o ~~~), si lo es.
fn fence_marker(line: &str) -> Option<&'static str> {
    let t = line.trim_start();
    if t.starts_with("```") {
        Some("```")
    } else if t.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

/// Itera las líneas con su offset en bytes, sin incluir el salto de línea.
fn lines_with_offsets(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut offset = 0;
    text.split_inclusive('\n').map(move |raw| {
        let start = offset;
        offset += raw.len();
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        (start, line)
    })
}

/// Parte el documento en bloques de Markdown y tareas, respetando los bloques de código.
pub fn split_blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut pending = String::new();
    let mut fence: Option<&'static str> = None;

    let flush = |pending: &mut String, blocks: &mut Vec<Block>| {
        if !pending.trim().is_empty() {
            blocks.push(Block::Markdown(std::mem::take(pending)));
        } else {
            pending.clear();
        }
    };

    for (start, line) in lines_with_offsets(text) {
        if let Some(open) = fence {
            pending.push_str(line);
            pending.push('\n');
            if line.trim_start().starts_with(open) {
                fence = None;
            }
            continue;
        }
        if let Some(marker) = fence_marker(line) {
            fence = Some(marker);
            pending.push_str(line);
            pending.push('\n');
            continue;
        }
        if let Some((indent, checked, mark, task_text)) = parse_task_line(line) {
            flush(&mut pending, &mut blocks);
            blocks.push(Block::Task(Task {
                indent,
                checked,
                mark: start + mark..start + mark + 1,
                text: task_text.to_string(),
            }));
            continue;
        }
        pending.push_str(line);
        pending.push('\n');
    }
    flush(&mut pending, &mut blocks);
    blocks
}

/// Rango en bytes de la línea que contiene `offset` (sin el salto de línea).
pub fn line_range_at(text: &str, offset: usize) -> Range<usize> {
    let offset = offset.min(text.len());
    let start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    let end = if text[start..end].ends_with('\r') { end - 1 } else { end };
    start..end
}

/// Edición a aplicar sobre el texto: reemplazar `range` por `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: &'static str,
}

/// Qué hacer con la línea en `offset` al pulsar el atajo de tarea:
/// - si es una tarea, alternar la marca;
/// - si es un ítem de lista, convertirlo en tarea;
/// - si es texto, convertirlo en `- [ ] texto`.
pub fn toggle_task_edit(text: &str, offset: usize) -> Edit {
    let line_range = line_range_at(text, offset);
    let line = &text[line_range.clone()];

    if let Some((_, checked, mark, _)) = parse_task_line(line) {
        let at = line_range.start + mark;
        return Edit {
            range: at..at + 1,
            text: if checked { " " } else { "x" },
        };
    }

    let trimmed = line.trim_start_matches([' ', '\t']);
    let lead = line.len() - trimmed.len();
    for bullet in ["- ", "* ", "+ "] {
        if trimmed.starts_with(bullet) {
            let at = line_range.start + lead + bullet.len();
            return Edit {
                range: at..at,
                text: "[ ] ",
            };
        }
    }
    let at = line_range.start + lead;
    Edit {
        range: at..at,
        text: "- [ ] ",
    }
}

/// Aplica una edición a una copia del texto (útil en tests y en la vista previa).
#[cfg(test)]
pub fn apply(text: &str, edit: &Edit) -> String {
    let mut out = String::with_capacity(text.len() + edit.text.len());
    out.push_str(&text[..edit.range.start]);
    out.push_str(edit.text);
    out.push_str(&text[edit.range.end..]);
    out
}

/// Cuenta `(hechas, total)` de tareas del documento, ignorando bloques de código.
pub fn progress(text: &str) -> (usize, usize) {
    split_blocks(text)
        .iter()
        .filter_map(|b| match b {
            Block::Task(t) => Some(t.checked),
            Block::Markdown(_) => None,
        })
        .fold((0, 0), |(done, total), checked| {
            (done + checked as usize, total + 1)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_variantes_de_tarea() {
        assert_eq!(parse_task_line("- [ ] a"), Some((0, false, 3, "a")));
        assert_eq!(parse_task_line("  * [x] hecho"), Some((2, true, 5, "hecho")));
        assert_eq!(parse_task_line("\t+ [X] b"), Some((4, true, 4, "b")));
        assert_eq!(parse_task_line("1. [ ] uno"), Some((0, false, 4, "uno")));
        assert_eq!(parse_task_line("- [ ]"), Some((0, false, 3, "")));
        assert_eq!(parse_task_line("- [y] no"), None);
        assert_eq!(parse_task_line("- [ ]pegado"), None);
        assert_eq!(parse_task_line("[ ] sin viñeta"), None);
        assert_eq!(parse_task_line("texto"), None);
    }

    #[test]
    fn separa_bloques_y_respeta_codigo() {
        let doc = "# Hoy\n- [ ] uno\n- [x] dos\n\n```\n- [ ] no es tarea\n```\nfin\n";
        let blocks = split_blocks(doc);
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0], Block::Markdown("# Hoy\n".into()));
        let Block::Task(t) = &blocks[1] else { panic!() };
        assert_eq!(&doc[t.mark.clone()], " ");
        assert!(!t.checked);
        let Block::Task(t) = &blocks[2] else { panic!() };
        assert_eq!(&doc[t.mark.clone()], "x");
        let Block::Markdown(code) = &blocks[3] else { panic!() };
        assert!(code.contains("- [ ] no es tarea"));
    }

    #[test]
    fn offsets_correctos_con_crlf_y_acentos() {
        let doc = "título ñ\r\n- [ ] café\r\n";
        let blocks = split_blocks(doc);
        let Block::Task(t) = &blocks[1] else { panic!() };
        assert_eq!(&doc[t.mark.clone()], " ");
        assert_eq!(t.text, "café");
    }

    #[test]
    fn alterna_convierte_y_agrega() {
        let doc = "- [ ] a\n- b\ntexto\n  - [x] c";
        let e = toggle_task_edit(doc, 2);
        assert_eq!(apply(doc, &e), "- [x] a\n- b\ntexto\n  - [x] c");
        let e = toggle_task_edit(doc, 9);
        assert_eq!(apply(doc, &e), "- [ ] a\n- [ ] b\ntexto\n  - [x] c");
        let e = toggle_task_edit(doc, 13);
        assert_eq!(apply(doc, &e), "- [ ] a\n- b\n- [ ] texto\n  - [x] c");
        let e = toggle_task_edit(doc, doc.len());
        assert_eq!(apply(doc, &e), "- [ ] a\n- b\ntexto\n  - [ ] c");
    }

    #[test]
    fn cuenta_progreso() {
        assert_eq!(progress("- [x] a\n- [ ] b\n- [X] c\n"), (2, 3));
        assert_eq!(progress("nada"), (0, 0));
    }
}
