//! Acciones de la app y atajos de teclado configurables.
//!
//! Los atajos se leen de `%APPDATA%\Notas\keymap.json`, un objeto
//! `{ "atajo": "Acción" }`. Poner una acción a `null` desactiva ese atajo.
//! Formato de las teclas (el de GPUI/Zed): `ctrl-s`, `ctrl-shift-p`, `alt-enter`, `f11`,
//! y secuencias separadas por espacio como `ctrl-k ctrl-s`.

use std::{collections::BTreeMap, fs, path::PathBuf};

use gpui::{App, KeyBinding, Keystroke, actions};

use crate::settings::data_dir;

/// Contexto de teclado de la ventana principal.
pub const CONTEXT: &str = "Notas";
/// Contexto de teclado del panel de ajustes.
pub const SETTINGS_CONTEXT: &str = "Ajustes";

actions!(
    notas,
    [
        NewFile,
        OpenFile,
        SearchNotes,
        CloseNote,
        Save,
        SaveAs,
        TogglePreview,
        TogglePin,
        ToggleMaximize,
        ToggleTask,
        ToggleTheme,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        OpenKeymap,
        OpenSettings,
        CloseSettings,
        Quit,
    ]
);

/// Atajos por defecto, en el orden en que se escriben en `keymap.json`.
const DEFAULTS: &[(&str, &str)] = &[
    ("ctrl-n", "NewFile"),
    ("ctrl-o", "OpenFile"),
    ("ctrl-k", "SearchNotes"),
    ("ctrl-w", "CloseNote"),
    ("ctrl-s", "Save"),
    ("ctrl-shift-s", "SaveAs"),
    ("ctrl-e", "TogglePreview"),
    ("ctrl-l", "ToggleTask"),
    ("ctrl-shift-t", "TogglePin"),
    ("f11", "ToggleMaximize"),
    ("ctrl-shift-d", "ToggleTheme"),
    ("ctrl-=", "ZoomIn"),
    ("ctrl-+", "ZoomIn"),
    ("ctrl--", "ZoomOut"),
    ("ctrl-0", "ZoomReset"),
    ("ctrl-,", "OpenSettings"),
    ("ctrl-shift-k", "OpenKeymap"),
    ("ctrl-q", "Quit"),
];

pub fn keymap_path() -> PathBuf {
    data_dir().join("keymap.json")
}

/// Crea una `KeyBinding` a partir del nombre de una acción.
fn binding(keys: &str, action: &str) -> Option<KeyBinding> {
    // `KeyBinding::new` entra en pánico con teclas inválidas: se validan antes.
    if keys.split_whitespace().any(|k| Keystroke::parse(k).is_err()) {
        return None;
    }
    let ctx = Some(CONTEXT);
    Some(match action {
        "NewFile" => KeyBinding::new(keys, NewFile, ctx),
        "OpenFile" => KeyBinding::new(keys, OpenFile, ctx),
        "SearchNotes" => KeyBinding::new(keys, SearchNotes, ctx),
        "CloseNote" => KeyBinding::new(keys, CloseNote, ctx),
        "Save" => KeyBinding::new(keys, Save, ctx),
        "SaveAs" => KeyBinding::new(keys, SaveAs, ctx),
        "TogglePreview" => KeyBinding::new(keys, TogglePreview, ctx),
        "TogglePin" => KeyBinding::new(keys, TogglePin, ctx),
        "ToggleMaximize" => KeyBinding::new(keys, ToggleMaximize, ctx),
        "ToggleTask" => KeyBinding::new(keys, ToggleTask, ctx),
        "ToggleTheme" => KeyBinding::new(keys, ToggleTheme, ctx),
        "ZoomIn" => KeyBinding::new(keys, ZoomIn, ctx),
        "ZoomOut" => KeyBinding::new(keys, ZoomOut, ctx),
        "ZoomReset" => KeyBinding::new(keys, ZoomReset, ctx),
        "OpenKeymap" => KeyBinding::new(keys, OpenKeymap, ctx),
        "OpenSettings" => KeyBinding::new(keys, OpenSettings, ctx),
        "Quit" => KeyBinding::new(keys, Quit, ctx),
        _ => return None,
    })
}

/// Escribe `keymap.json` con los atajos por defecto si todavía no existe.
pub fn ensure_keymap_file() {
    let path = keymap_path();
    if path.exists() {
        return;
    }
    let _ = fs::create_dir_all(data_dir());
    let mut json = String::from("{\n");
    for (i, (keys, action)) in DEFAULTS.iter().enumerate() {
        let comma = if i + 1 < DEFAULTS.len() { "," } else { "" };
        json.push_str(&format!("  \"{keys}\": \"{action}\"{comma}\n"));
    }
    json.push_str("}\n");
    let _ = fs::write(path, json);
}

/// Registra los atajos: los por defecto, sobrescritos por los de `keymap.json`.
/// Se llama una vez al arrancar; los cambios en el archivo se aplican al reiniciar.
/// Devuelve los errores encontrados en el archivo del usuario, para mostrarlos.
pub fn load(cx: &mut App) -> Vec<String> {
    let mut map: BTreeMap<String, Option<String>> = DEFAULTS
        .iter()
        .map(|(k, a)| (k.to_string(), Some(a.to_string())))
        .collect();
    let mut errors = Vec::new();

    if let Ok(src) = fs::read_to_string(keymap_path()) {
        match serde_json::from_str::<BTreeMap<String, Option<String>>>(&src) {
            Ok(user) => map.extend(user),
            Err(e) => errors.push(format!("keymap.json: {e}")),
        }
    }

    // Fijo: dentro del panel, una lista desplegable abierta se cierra antes con Esc
    // porque su contexto es más interno.
    let mut bindings = vec![KeyBinding::new("escape", CloseSettings, Some(SETTINGS_CONTEXT))];
    for (keys, action) in &map {
        let Some(action) = action else { continue };
        match binding(keys, action) {
            Some(b) => bindings.push(b),
            None => errors.push(format!("atajo inválido: \"{keys}\": \"{action}\"")),
        }
    }
    cx.bind_keys(bindings);
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todas_las_acciones_por_defecto_existen() {
        for (keys, action) in DEFAULTS {
            assert!(binding(keys, action).is_some(), "{keys} -> {action}");
        }
    }
}
