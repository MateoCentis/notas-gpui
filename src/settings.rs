//! Preferencias persistentes en `%APPDATA%\Notas\settings.json`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// Carpeta de datos de la app (`%APPDATA%\Notas`).
pub fn data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Notas")
}

/// Carpeta de notas por defecto: `Documentos\Notas`.
fn default_notes_dir() -> PathBuf {
    dirs::document_dir()
        .map(|d| d.join("Notas"))
        .unwrap_or_else(|| data_dir().join("notas"))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SavedBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub maximized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Nombre del tema (ver `theme.rs`).
    pub theme: String,
    /// Variante oscura del tema.
    pub dark: bool,
    /// Tamaño de letra del editor en píxeles.
    pub font_size: f32,
    /// Fuente del editor; `null` usa la monoespaciada del sistema (Consolas).
    pub font_family: Option<String>,
    /// Fuente de la interfaz y de la vista; `null` usa la del sistema (Segoe UI).
    pub ui_font_family: Option<String>,
    /// Ventana siempre encima de las demás.
    pub always_on_top: bool,
    /// Guardar automáticamente al dejar de escribir.
    pub autosave: bool,
    /// Milisegundos de inactividad antes del autoguardado.
    pub autosave_delay_ms: u64,
    /// Abrir en modo vista (Markdown renderizado) en lugar de edición.
    pub start_in_preview: bool,
    /// Carpeta donde se guardan las notas; `null` usa `Documentos\Notas`.
    pub notes_dir: Option<PathBuf>,
    /// Notas abiertas al cerrar, de la menos a la más reciente.
    pub open_notes: Vec<PathBuf>,
    /// Posición y tamaño de la ventana la última vez que se cerró.
    pub window: Option<SavedBounds>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: crate::theme::DEFAULT.into(),
            dark: true,
            font_size: 15.0,
            font_family: None,
            ui_font_family: None,
            always_on_top: false,
            autosave: true,
            autosave_delay_ms: 800,
            start_in_preview: false,
            notes_dir: None,
            open_notes: Vec::new(),
            window: None,
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        data_dir().join("settings.json")
    }

    /// Carga las preferencias; si no existen o están dañadas usa los valores por defecto.
    pub fn load() -> Self {
        Self::read().ok().flatten().unwrap_or_default()
    }

    /// Lee `settings.json`: `Ok(None)` si no existe, `Err` si no es válido.
    pub fn read() -> Result<Option<Self>, String> {
        let Ok(src) = fs::read_to_string(Self::path()) else {
            return Ok(None);
        };
        serde_json::from_str(&src)
            .map(Some)
            .map_err(|e| format!("settings.json: {e}"))
    }

    pub fn save(&self) {
        let _ = fs::create_dir_all(data_dir());
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(Self::path(), json);
        }
    }

    pub fn notes_dir(&self) -> PathBuf {
        self.notes_dir.clone().unwrap_or_else(default_notes_dir)
    }

    /// Crea la carpeta de notas y mueve allí la nota única de la versión anterior.
    pub fn prepare_notes_dir(&self) {
        let dir = self.notes_dir();
        let _ = fs::create_dir_all(&dir);
        let old = data_dir().join("notas.md");
        let new = dir.join("notas.md");
        if old.exists() && !new.exists() {
            move_file(&old, &new);
        }
    }
}

/// Mueve un archivo, aunque esté en otra unidad.
fn move_file(from: &Path, to: &Path) {
    if fs::rename(from, to).is_err() && fs::copy(from, to).is_ok() {
        let _ = fs::remove_file(from);
    }
}
