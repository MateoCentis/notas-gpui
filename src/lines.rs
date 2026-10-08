//! Lógica pura de edición por líneas: borrar las líneas de la selección y
//! moverlas hacia arriba o hacia abajo.

use std::ops::Range;

/// Una edición: reemplazar `range` por `text` y dejar seleccionado `selection`
/// (rango en bytes sobre el texto ya editado).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEdit {
    pub range: Range<usize>,
    pub text: String,
    pub selection: Range<usize>,
}

/// Comienzo de la primera línea y fin de la última (sin el `\n`) que toca `sel`.
/// Una selección que termina justo al comienzo de una línea no la incluye.
fn line_span(text: &str, sel: &Range<usize>) -> Range<usize> {
    let start = text[..sel.start].rfind('\n').map_or(0, |i| i + 1);
    let anchor = if sel.end > sel.start && text[..sel.end].ends_with('\n') {
        sel.end - 1
    } else {
        sel.end
    };
    let end = text[anchor..].find('\n').map_or(text.len(), |i| anchor + i);
    start..end
}

/// Borra las líneas que toca la selección. El cursor queda en la línea siguiente
/// (o en la anterior si se borró la última), en la misma columna si existe.
pub fn delete_lines(text: &str, sel: Range<usize>) -> LineEdit {
    let span = line_span(text, &sel);
    let column = sel.start - span.start;
    // Llevarse también un salto de línea: el de después o, si es la última, el de antes.
    let (range, line_start) = if span.end < text.len() {
        (span.start..span.end + 1, span.start)
    } else if span.start > 0 {
        let prev = text[..span.start - 1].rfind('\n').map_or(0, |i| i + 1);
        (span.start - 1..span.end, prev)
    } else {
        (span.clone(), 0)
    };

    let after = format!("{}{}", &text[..range.start], &text[range.end..]);
    let line_end = after[line_start..]
        .find('\n')
        .map_or(after.len(), |i| line_start + i);
    let mut cursor = (line_start + column).min(line_end);
    while !after.is_char_boundary(cursor) {
        cursor -= 1;
    }
    LineEdit {
        range,
        text: String::new(),
        selection: cursor..cursor,
    }
}

/// Mueve las líneas que toca la selección una posición arriba o abajo, conservando
/// la selección sobre ellas. `None` si ya están en el borde del documento.
pub fn move_lines(text: &str, sel: Range<usize>, up: bool) -> Option<LineEdit> {
    let span = line_span(text, &sel);
    let lines = &text[span.clone()];
    if up {
        if span.start == 0 {
            return None;
        }
        let prev = text[..span.start - 1].rfind('\n').map_or(0, |i| i + 1);
        let shift = span.start - prev;
        Some(LineEdit {
            range: prev..span.end,
            text: format!("{lines}\n{}", &text[prev..span.start - 1]),
            selection: sel.start - shift..sel.end - shift,
        })
    } else {
        if span.end == text.len() {
            return None;
        }
        let next_end = text[span.end + 1..]
            .find('\n')
            .map_or(text.len(), |i| span.end + 1 + i);
        let shift = next_end - span.end;
        Some(LineEdit {
            range: span.start..next_end,
            text: format!("{}\n{lines}", &text[span.end + 1..next_end]),
            selection: sel.start + shift..sel.end + shift,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, edit: &LineEdit) -> String {
        format!(
            "{}{}{}",
            &text[..edit.range.start],
            edit.text,
            &text[edit.range.end..]
        )
    }

    #[test]
    fn borra_la_linea_del_cursor() {
        let text = "uno\ndos\ntres";
        let edit = delete_lines(text, 5..5);
        assert_eq!(apply(text, &edit), "uno\ntres");
        assert_eq!(edit.selection, 5..5);
    }

    #[test]
    fn borra_la_ultima_linea_y_sube() {
        let text = "uno\ndos\ntres";
        let edit = delete_lines(text, 10..10);
        assert_eq!(apply(text, &edit), "uno\ndos");
        assert_eq!(edit.selection, 6..6);
    }

    #[test]
    fn borra_la_unica_linea() {
        let edit = delete_lines("solo", 2..2);
        assert_eq!(apply("solo", &edit), "");
        assert_eq!(edit.selection, 0..0);
    }

    #[test]
    fn borra_todas_las_lineas_seleccionadas() {
        let text = "uno\ndos\ntres\ncuatro";
        let edit = delete_lines(text, 5..10);
        assert_eq!(apply(text, &edit), "uno\ncuatro");
    }

    #[test]
    fn la_columna_se_ajusta_a_una_linea_mas_corta() {
        let text = "largo largo\nx\n";
        let edit = delete_lines(text, 8..8);
        assert_eq!(apply(text, &edit), "x\n");
        assert_eq!(edit.selection, 1..1);
    }

    #[test]
    fn la_columna_respeta_caracteres_multibyte() {
        let text = "abcd\nñ";
        let edit = delete_lines(text, 1..1);
        assert_eq!(apply(text, &edit), "ñ");
        assert_eq!(edit.selection, 0..0);
    }

    #[test]
    fn mueve_una_linea_arriba_y_abajo() {
        let text = "uno\ndos\ntres";
        let up = move_lines(text, 5..5, true).unwrap();
        assert_eq!(apply(text, &up), "dos\nuno\ntres");
        assert_eq!(up.selection, 1..1);

        let down = move_lines(text, 5..5, false).unwrap();
        assert_eq!(apply(text, &down), "uno\ntres\ndos");
        assert_eq!(down.selection, 10..10);
    }

    #[test]
    fn mueve_varias_lineas_con_la_seleccion() {
        let text = "a\nb\nc\nd";
        let edit = move_lines(text, 2..5, false).unwrap();
        assert_eq!(apply(text, &edit), "a\nd\nb\nc");
        assert_eq!(&apply(text, &edit)[edit.selection], "b\nc");
    }

    #[test]
    fn una_seleccion_que_termina_al_inicio_de_linea_no_la_incluye() {
        let text = "a\nb\nc";
        let edit = move_lines(text, 0..2, false).unwrap();
        assert_eq!(apply(text, &edit), "b\na\nc");
    }

    #[test]
    fn no_mueve_mas_alla_de_los_bordes() {
        assert!(move_lines("uno\ndos", 1..1, true).is_none());
        assert!(move_lines("uno\ndos", 5..5, false).is_none());
    }
}
