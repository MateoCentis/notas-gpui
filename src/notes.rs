//! Biblioteca de notas: cada nota es un `.md` en la carpeta de notas.
//! Aquí está la lógica sin interfaz: títulos, nombres de archivo y búsqueda.

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Extensiones que se listan como notas.
const EXTENSIONS: &[&str] = &["md", "markdown", "txt"];
/// Bytes que se leen de cada nota para buscar en su contenido.
const SCAN_BYTES: usize = 16 * 1024;

/// Título de una nota: su primera línea con texto, sin `#`, viñetas ni casillas.
pub fn title_of(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = line.trim_start_matches('#').trim_start();
    let line = line
        .strip_prefix("- [ ] ")
        .or_else(|| line.strip_prefix("- [x] "))
        .or_else(|| line.strip_prefix("- "))
        .or_else(|| line.strip_prefix("* "))
        .unwrap_or(line)
        .trim();
    if line.is_empty() {
        return None;
    }
    let mut title: String = line.chars().take(60).collect();
    if line.chars().count() > 60 {
        title.push('…');
    }
    Some(title)
}

/// Título a mostrar: el del contenido o, si está vacía, el nombre del archivo.
pub fn display_title(text: &str, path: Option<&Path>) -> String {
    title_of(text)
        .or_else(|| {
            path.and_then(Path::file_stem)
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "Nota nueva".into())
}

/// Minúsculas y sin tildes, para comparar "Cafe" con "café".
pub fn normalize(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// Nombre de archivo seguro a partir del título: `Tareas de hoy` → `tareas-de-hoy`.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in normalize(title).chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 48 {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "nota".into() } else { out }
}

/// Ruta libre en `dir` para una nota con ese título (`slug.md`, `slug-2.md`, …).
pub fn unique_path(dir: &Path, title: &str) -> PathBuf {
    let base = slug(title);
    let mut path = dir.join(format!("{base}.md"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{base}-{n}.md"));
        n += 1;
    }
    path
}

/// Lee un archivo como texto: quita el BOM y normaliza `\r\n` a `\n`.
/// Devuelve `(texto, usaba_crlf, aviso)`.
pub fn read_file(path: &Path) -> std::io::Result<(String, bool, Option<&'static str>)> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        // Un archivo que no existe se crea al guardar.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e),
    };
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    let (text, warning) = match String::from_utf8(bytes.to_vec()) {
        Ok(t) => (t, None),
        Err(_) => (
            String::from_utf8_lossy(bytes).into_owned(),
            Some("El archivo no es UTF-8; algunos caracteres pueden verse mal"),
        ),
    };
    let crlf = text.contains("\r\n");
    let text = if crlf {
        text.replace("\r\n", "\n")
    } else {
        text
    };
    Ok((text, crlf, warning))
}

/// Escribe el texto en disco de forma atómica: si algo falla, el original queda intacto.
pub fn write_file(path: &Path, text: &str, crlf: bool) -> std::io::Result<()> {
    let text = if crlf {
        text.replace('\n', "\r\n")
    } else {
        text.to_string()
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("notas-tmp");
    std::fs::write(&tmp, text.as_bytes())
        .and_then(|_| std::fs::rename(&tmp, path))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
}

/// Una nota encontrada en disco.
#[derive(Debug, Clone)]
pub struct NoteFile {
    pub path: PathBuf,
    pub title: String,
    /// Título y contenido normalizados, para buscar.
    pub title_norm: String,
    pub body_norm: String,
    pub modified: Option<SystemTime>,
}

impl NoteFile {
    pub fn from_text(path: PathBuf, text: &str, modified: Option<SystemTime>) -> Self {
        let title = display_title(text, Some(&path));
        Self {
            title_norm: normalize(&title),
            body_norm: normalize(text),
            title,
            path,
            modified,
        }
    }
}

/// Lee las notas de la carpeta, de la más reciente a la más antigua.
pub fn scan(dir: &Path) -> Vec<NoteFile> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut notes: Vec<NoteFile> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        })
        .map(|path| {
            let modified = fs::metadata(&path).and_then(|m| m.modified()).ok();
            let bytes = fs::read(&path).unwrap_or_default();
            let bytes = &bytes[..bytes.len().min(SCAN_BYTES)];
            let text = String::from_utf8_lossy(bytes);
            NoteFile::from_text(path, &text, modified)
        })
        .collect();
    notes.sort_by_key(|n| std::cmp::Reverse(n.modified));
    notes
}

/// Qué tan bien coincide una nota con la búsqueda; `None` si no coincide.
/// Menor es mejor: 0 = todas las palabras en el título, 1 = alguna solo en el contenido.
pub fn match_score(query_norm: &str, title_norm: &str, body_norm: &str) -> Option<u8> {
    let words: Vec<&str> = query_norm.split_whitespace().collect();
    if words.is_empty() {
        return Some(0);
    }
    if words.iter().all(|w| title_norm.contains(w)) {
        return Some(0);
    }
    words
        .iter()
        .all(|w| title_norm.contains(w) || body_norm.contains(w))
        .then_some(1)
}

/// "hace 5 min", "hace 3 h", "hace 2 d".
pub fn relative_time(t: SystemTime) -> String {
    let secs = SystemTime::now()
        .duration_since(t)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match secs {
        0..60 => "ahora".into(),
        60..3600 => format!("hace {} min", secs / 60),
        3600..86400 => format!("hace {} h", secs / 3600),
        86400..2592000 => format!("hace {} d", secs / 86400),
        _ => format!("hace {} meses", secs / 2592000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titulos() {
        assert_eq!(
            title_of("\n\n# Tareas de hoy\n- a").as_deref(),
            Some("Tareas de hoy")
        );
        assert_eq!(
            title_of("- [ ] Comprar pan").as_deref(),
            Some("Comprar pan")
        );
        assert_eq!(title_of("   \n  "), None);
        assert_eq!(title_of("#"), None);
        assert_eq!(display_title("", Some(Path::new("C:/n/ideas.md"))), "ideas");
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Tareas de hoy"), "tareas-de-hoy");
        assert_eq!(slug("¿Qué hacer? Mañana!"), "que-hacer-manana");
        assert_eq!(slug("***"), "nota");
    }

    #[test]
    fn busqueda_ignora_tildes_y_mayusculas() {
        let t = normalize("Café con Ñandú");
        assert_eq!(t, "cafe con nandu");
        assert_eq!(match_score("cafe", &t, ""), Some(0));
        assert_eq!(match_score("nandu cafe", &t, ""), Some(0));
        assert_eq!(match_score("leche", &t, "comprar leche"), Some(1));
        assert_eq!(match_score("leche", &t, "nada"), None);
        assert_eq!(match_score("", &t, ""), Some(0));
    }

    #[test]
    fn rutas_unicas() {
        let dir = std::env::temp_dir().join(format!("notas-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let a = unique_path(&dir, "Hola");
        assert_eq!(a.file_name().unwrap(), "hola.md");
        fs::write(&a, "x").unwrap();
        assert_eq!(unique_path(&dir, "Hola").file_name().unwrap(), "hola-2.md");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn lee_y_escribe_respetando_bom_y_crlf() {
        let dir = std::env::temp_dir().join(format!("notas-test-io-{}", std::process::id()));
        let path = dir.join("a.md");
        assert_eq!(read_file(&path).unwrap(), (String::new(), false, None));

        write_file(&path, "uno\ndos\n", true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"uno\r\ndos\r\n");
        assert_eq!(read_file(&path).unwrap(), ("uno\ndos\n".into(), true, None));

        fs::write(&path, b"\xEF\xBB\xBFhola").unwrap();
        assert_eq!(read_file(&path).unwrap(), ("hola".into(), false, None));

        fs::write(&path, b"caf\xE9").unwrap();
        assert!(read_file(&path).unwrap().2.is_some());
        fs::remove_dir_all(&dir).unwrap();
    }
}
