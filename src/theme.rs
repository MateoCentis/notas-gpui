//! Temas de colores y fuentes.
//!
//! Los temas incluidos están en `assets/themes`. Cada `.json` de
//! `%APPDATA%\Notas\themes` (con el mismo formato) agrega uno, o reemplaza al
//! incluido que tenga el mismo nombre. Cada tema tiene variante clara y oscura.

use std::{path::PathBuf, rc::Rc};

use gpui::{App, Global, SharedString, Window};
use gpui_kit::component::{Theme, ThemeConfig, ThemeMode, ThemeSet};

use crate::settings::{Settings, data_dir};

/// Tema por defecto.
pub const DEFAULT: &str = "Notas";

/// Familia que GPUI resuelve a la fuente de interfaz del sistema (Segoe UI).
const SYSTEM_UI_FONT: &str = ".SystemUIFont";

const BUILTIN: &[&str] = &[
    include_str!("../assets/themes/notas.json"),
    include_str!("../assets/themes/bosque.json"),
    include_str!("../assets/themes/oceano.json"),
    include_str!("../assets/themes/sepia.json"),
    include_str!("../assets/themes/grafito.json"),
];

/// Un tema con sus dos variantes. Si el archivo trae solo una, se usa para ambas.
struct Entry {
    name: SharedString,
    light: Rc<ThemeConfig>,
    dark: Rc<ThemeConfig>,
}

/// Temas disponibles y fuentes instaladas.
#[derive(Default)]
struct Catalog {
    themes: Vec<Entry>,
    fonts: Vec<SharedString>,
}

impl Global for Catalog {}

pub fn themes_dir() -> PathBuf {
    data_dir().join("themes")
}

fn parse(src: &str) -> Result<Entry, String> {
    let set: ThemeSet = serde_json::from_str(src).map_err(|e| e.to_string())?;
    let pick = |dark: bool| {
        set.themes
            .iter()
            .find(|t| t.mode.is_dark() == dark)
            .cloned()
            .map(Rc::new)
    };
    let (light, dark) = match (pick(false), pick(true)) {
        (Some(l), Some(d)) => (l, d),
        (Some(t), None) | (None, Some(t)) => (t.clone(), t),
        (None, None) => return Err("no tiene ningún tema".into()),
    };
    Ok(Entry {
        name: set.name,
        light,
        dark,
    })
}

/// Archivos de tema del usuario, en orden alfabético.
fn user_theme_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(themes_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("json"))
        })
        .collect();
    files.sort();
    // Formato anterior: un único `theme.json` en la carpeta de datos.
    let legacy = data_dir().join("theme.json");
    if legacy.exists() {
        files.insert(0, legacy);
    }
    files
}

/// (Re)lee los temas y la lista de fuentes instaladas.
/// Devuelve los errores de los archivos del usuario, para mostrarlos.
pub fn load(cx: &mut App) -> Vec<String> {
    let mut themes: Vec<Entry> = BUILTIN
        .iter()
        .map(|src| parse(src).expect("tema incorporado inválido"))
        .collect();
    let mut errors = Vec::new();

    for path in user_theme_files() {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let entry = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|src| parse(&src));
        match entry {
            Ok(entry) => match themes.iter_mut().find(|t| t.name == entry.name) {
                Some(existing) => *existing = entry,
                None => themes.push(entry),
            },
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }

    let fonts = cx
        .text_system()
        .all_font_names()
        .into_iter()
        // Las familias virtuales de GPUI (`.SystemUIFont`, `.ZedMono`…) empiezan por punto.
        .filter(|f| !f.starts_with('.'))
        .map(SharedString::from)
        .collect();

    cx.set_global(Catalog { themes, fonts });
    errors
}

/// Nombres de los temas disponibles.
pub fn names(cx: &App) -> Vec<SharedString> {
    cx.global::<Catalog>()
        .themes
        .iter()
        .map(|t| t.name.clone())
        .collect()
}

/// Fuentes instaladas, ordenadas y sin repetir.
pub fn fonts(cx: &App) -> Vec<SharedString> {
    cx.global::<Catalog>().fonts.clone()
}

/// Devuelve la familia tal como la conoce el sistema, si está instalada.
/// GPUI se cae al dibujar texto con una fuente que no existe, así que toda
/// fuente elegida por el usuario pasa por aquí antes de usarse.
pub fn installed_font(family: &str, cx: &App) -> Option<SharedString> {
    cx.global::<Catalog>()
        .fonts
        .iter()
        .find(|f| f.eq_ignore_ascii_case(family))
        .cloned()
}

/// Aplica el tema, su variante y la fuente de la interfaz de los ajustes.
/// Devuelve un aviso por cada ajuste que no se pudo aplicar.
pub fn apply(settings: &Settings, window: Option<&mut Window>, cx: &mut App) -> Vec<String> {
    let mut errors = Vec::new();
    let catalog = cx.global::<Catalog>();
    let find = |name: &str| catalog.themes.iter().find(|t| t.name == name);
    let entry = find(&settings.theme).unwrap_or_else(|| {
        errors.push(format!("No existe el tema «{}»", settings.theme));
        find(DEFAULT).expect("falta el tema por defecto")
    });
    let (light, dark) = (entry.light.clone(), entry.dark.clone());

    let ui_font = match settings.ui_font_family.as_deref() {
        None => SharedString::from(SYSTEM_UI_FONT),
        Some(family) => installed_font(family, cx).unwrap_or_else(|| {
            errors.push(format!("La fuente «{family}» no está instalada"));
            SYSTEM_UI_FONT.into()
        }),
    };

    let theme = Theme::global_mut(cx);
    theme.light_theme = light;
    theme.dark_theme = dark;
    let mode = if settings.dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    Theme::change(mode, window, cx);
    // Después del cambio de tema, por si el archivo del tema trae su propia fuente.
    Theme::update(cx, |theme| theme.font_family = ui_font);
    errors
}

#[cfg(test)]
mod tests {
    #[test]
    fn los_temas_incorporados_son_validos() {
        let mut names = Vec::new();
        for src in super::BUILTIN {
            let entry = super::parse(src).expect("tema inválido");
            assert!(entry.dark.mode.is_dark(), "{}", entry.name);
            assert!(!entry.light.mode.is_dark(), "{}", entry.name);
            assert!(entry.light.highlight.is_some() && entry.dark.highlight.is_some());
            names.push(entry.name);
        }
        assert!(names.iter().any(|n| n == super::DEFAULT));
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
    }
}
