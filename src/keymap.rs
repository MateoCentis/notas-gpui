//! Acciones de la app y atajos de teclado configurables.
//!
//! Los atajos se leen de `%APPDATA%\Notas\keymap.json`, un objeto
//! `{ "atajo": "Acción" }`. Poner una acción a `null` desactiva ese atajo.
//! Formato de las teclas (el de GPUI/Zed): `ctrl-s`, `ctrl-shift-p`, `alt-enter`, `f11`,
//! y secuencias separadas por espacio como `ctrl-k ctrl-s`.
//! Se pueden cambiar desde el panel de ajustes o editando el archivo; en ambos
//! casos se aplican al momento.

use std::{collections::BTreeMap, fs, path::PathBuf};

use gpui::{App, Global, KeyBinding, KeyBindingMetaIndex, Keystroke, actions};

use crate::settings::data_dir;

/// Contexto de teclado de la ventana principal.
pub const CONTEXT: &str = "Notas";
/// Contexto de teclado del panel de ajustes.
pub const SETTINGS_CONTEXT: &str = "Ajustes";

/// Marca los atajos de la app, para poder quitarlos sin tocar los de los componentes.
const META: KeyBindingMetaIndex = KeyBindingMetaIndex(0x4e6f);

actions!(
    notas,
    [
        NewFile,
        OpenFile,
        SearchNotes,
        CloseNote,
        DeleteNote,
        Save,
        SaveAs,
        TogglePreview,
        TogglePin,
        ToggleMaximize,
        ToggleTask,
        ToggleTheme,
        DeleteLine,
        MoveLineUp,
        MoveLineDown,
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
    ("ctrl-alt-w", "DeleteNote"),
    ("ctrl-s", "Save"),
    ("ctrl-shift-s", "SaveAs"),
    ("ctrl-e", "TogglePreview"),
    ("ctrl-l", "ToggleTask"),
    ("ctrl-d", "DeleteLine"),
    ("alt-up", "MoveLineUp"),
    ("alt-down", "MoveLineDown"),
    ("ctrl-shift-t", "TogglePin"),
    ("f11", "ToggleMaximize"),
    ("ctrl-t", "ToggleTheme"),
    ("ctrl-=", "ZoomIn"),
    ("ctrl-+", "ZoomIn"),
    ("ctrl--", "ZoomOut"),
    ("ctrl-0", "ZoomReset"),
    ("ctrl-,", "OpenSettings"),
    ("ctrl-shift-k", "OpenKeymap"),
    ("ctrl-q", "Quit"),
];

/// Atajos por defecto de versiones anteriores que cambiaron. `keymap.json` se crea
/// con los atajos por defecto, así que estos se quitan de él al arrancar.
const OLD_DEFAULTS: &[(&str, &str)] = &[("ctrl-shift-d", "ToggleTheme")];

/// Acciones que se pueden asignar desde el panel de ajustes, en su orden.
pub const ACTIONS: &[(&str, &str)] = &[
    ("NewFile", "Nueva nota"),
    ("OpenFile", "Abrir archivo"),
    ("SearchNotes", "Buscar notas"),
    ("Save", "Guardar"),
    ("SaveAs", "Guardar como"),
    ("CloseNote", "Cerrar nota"),
    ("DeleteNote", "Borrar nota (a la papelera)"),
    ("TogglePreview", "Edición / vista"),
    ("ToggleTask", "Crear / marcar tarea"),
    ("DeleteLine", "Borrar línea"),
    ("MoveLineUp", "Mover línea arriba"),
    ("MoveLineDown", "Mover línea abajo"),
    ("ToggleTheme", "Claro / oscuro"),
    ("TogglePin", "Siempre encima"),
    ("ToggleMaximize", "Maximizar"),
    ("ZoomIn", "Agrandar letra"),
    ("ZoomOut", "Achicar letra"),
    ("ZoomReset", "Tamaño de letra normal"),
    ("OpenSettings", "Ajustes"),
    ("OpenKeymap", "Editar keymap.json"),
    ("Quit", "Salir"),
];

type Map = BTreeMap<String, Option<String>>;

/// Atajos en uso (los por defecto más los de `keymap.json`).
#[derive(Default)]
struct Shortcuts(Map);

impl Global for Shortcuts {}

pub fn keymap_path() -> PathBuf {
    data_dir().join("keymap.json")
}

/// Crea una `KeyBinding` a partir del nombre de una acción.
fn binding(keys: &str, action: &str) -> Option<KeyBinding> {
    // `KeyBinding::new` entra en pánico con teclas inválidas: se validan antes.
    if keys
        .split_whitespace()
        .any(|k| Keystroke::parse(k).is_err())
    {
        return None;
    }
    let ctx = Some(CONTEXT);
    let binding = match action {
        "NewFile" => KeyBinding::new(keys, NewFile, ctx),
        "OpenFile" => KeyBinding::new(keys, OpenFile, ctx),
        "SearchNotes" => KeyBinding::new(keys, SearchNotes, ctx),
        "CloseNote" => KeyBinding::new(keys, CloseNote, ctx),
        "DeleteNote" => KeyBinding::new(keys, DeleteNote, ctx),
        "Save" => KeyBinding::new(keys, Save, ctx),
        "SaveAs" => KeyBinding::new(keys, SaveAs, ctx),
        "TogglePreview" => KeyBinding::new(keys, TogglePreview, ctx),
        "TogglePin" => KeyBinding::new(keys, TogglePin, ctx),
        "ToggleMaximize" => KeyBinding::new(keys, ToggleMaximize, ctx),
        "ToggleTask" => KeyBinding::new(keys, ToggleTask, ctx),
        "ToggleTheme" => KeyBinding::new(keys, ToggleTheme, ctx),
        "DeleteLine" => KeyBinding::new(keys, DeleteLine, ctx),
        "MoveLineUp" => KeyBinding::new(keys, MoveLineUp, ctx),
        "MoveLineDown" => KeyBinding::new(keys, MoveLineDown, ctx),
        "ZoomIn" => KeyBinding::new(keys, ZoomIn, ctx),
        "ZoomOut" => KeyBinding::new(keys, ZoomOut, ctx),
        "ZoomReset" => KeyBinding::new(keys, ZoomReset, ctx),
        "OpenKeymap" => KeyBinding::new(keys, OpenKeymap, ctx),
        "OpenSettings" => KeyBinding::new(keys, OpenSettings, ctx),
        "Quit" => KeyBinding::new(keys, Quit, ctx),
        _ => return None,
    };
    Some(binding.with_meta(META))
}

/// Escribe `keymap.json` con los atajos por defecto si todavía no existe; si existe,
/// le quita los atajos por defecto que ya no lo son.
pub fn ensure_keymap_file() {
    let path = keymap_path();
    if path.exists() {
        if let Ok(mut user) = read_user() {
            let before = user.len();
            for (keys, action) in OLD_DEFAULTS {
                if user
                    .get(*keys)
                    .is_some_and(|a| a.as_deref() == Some(*action))
                {
                    user.remove(*keys);
                }
            }
            if user.len() != before {
                let _ = write_user(&user);
            }
        }
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

/// Lee los atajos del usuario: `Ok(vacío)` si el archivo no existe.
fn read_user() -> Result<Map, String> {
    match fs::read_to_string(keymap_path()) {
        Ok(src) => serde_json::from_str(&src).map_err(|e| format!("keymap.json: {e}")),
        Err(_) => Ok(Map::new()),
    }
}

fn write_user(map: &Map) -> Result<(), String> {
    let _ = fs::create_dir_all(data_dir());
    let json = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    fs::write(keymap_path(), json + "\n").map_err(|e| format!("keymap.json: {e}"))
}

/// Registra los atajos: los por defecto, sobrescritos por los de `keymap.json`.
/// Se puede volver a llamar para aplicar cambios: reemplaza los atajos anteriores.
/// Devuelve los errores encontrados en el archivo del usuario, para mostrarlos.
pub fn load(cx: &mut App) -> Vec<String> {
    let mut map: Map = DEFAULTS
        .iter()
        .map(|(k, a)| (k.to_string(), Some(a.to_string())))
        .collect();
    let mut errors = Vec::new();
    match read_user() {
        Ok(user) => map.extend(user),
        Err(e) => errors.push(e),
    }

    // Fijo: dentro del panel, una lista desplegable abierta se cierra antes con Esc
    // porque su contexto es más interno.
    let mut bindings =
        vec![KeyBinding::new("escape", CloseSettings, Some(SETTINGS_CONTEXT)).with_meta(META)];
    for (keys, action) in &map {
        let Some(action) = action else { continue };
        match binding(keys, action) {
            Some(b) => bindings.push(b),
            None => errors.push(format!("atajo inválido: \"{keys}\": \"{action}\"")),
        }
    }

    // Conservar los atajos de los componentes (editor, listas…) y cambiar solo los propios.
    let others: Vec<KeyBinding> = cx
        .key_bindings()
        .borrow()
        .bindings()
        .filter(|b| b.meta() != Some(META))
        .cloned()
        .collect();
    cx.clear_key_bindings();
    cx.bind_keys(others);
    cx.bind_keys(bindings);
    cx.set_global(Shortcuts(map));
    errors
}

/// Atajos asignados a una acción, en formato de `keymap.json`.
pub fn keys_for(action: &str, cx: &App) -> Vec<String> {
    cx.try_global::<Shortcuts>()
        .map(|s| {
            s.0.iter()
                .filter(|(_, a)| a.as_deref() == Some(action))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Primer atajo de una acción, para mostrarlo: `Ctrl+Shift+T`.
pub fn label(action: &str, cx: &App) -> Option<String> {
    keys_for(action, cx).first().map(|k| display(k))
}

/// Texto de ayuda con el atajo entre paréntesis, si la acción tiene uno.
pub fn hint(text: &str, action: &str, cx: &App) -> String {
    match label(action, cx) {
        Some(keys) => format!("{text} ({keys})"),
        None => text.to_string(),
    }
}

/// Convierte `ctrl-shift-t` en `Ctrl+Shift+T`, para mostrar.
pub fn display(keys: &str) -> String {
    keys.split_whitespace()
        .map(|source| {
            let Ok(k) = Keystroke::parse(source) else {
                return source.to_string();
            };
            let m = k.modifiers;
            let mut parts: Vec<String> = [
                (m.control, "Ctrl"),
                (m.alt, "Alt"),
                (m.platform, "Win"),
                (m.shift, "Shift"),
            ]
            .into_iter()
            .filter(|(on, _)| *on)
            .map(|(_, name)| name.to_string())
            .collect();
            parts.push(match k.key.as_str() {
                "up" => "↑".into(),
                "down" => "↓".into(),
                "left" => "←".into(),
                "right" => "→".into(),
                "escape" => "Esc".into(),
                "delete" => "Supr".into(),
                "space" => "Espacio".into(),
                key => {
                    let mut chars = key.chars();
                    chars.next().map_or(String::new(), |c| {
                        c.to_uppercase().chain(chars).collect::<String>()
                    })
                }
            });
            parts.join("+")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Nombre visible de una acción.
fn action_name(action: &str) -> &str {
    ACTIONS
        .iter()
        .find(|(a, _)| *a == action)
        .map_or(action, |(_, name)| name)
}

/// Asigna `keys` a `action` en `keymap.json` (o le quita el atajo con `None`) y lo aplica.
/// Devuelve un aviso si el atajo estaba asignado a otra acción.
pub fn set_shortcut(
    action: &str,
    keys: Option<&str>,
    cx: &mut App,
) -> Result<Option<String>, String> {
    let mut user = read_user()?;
    let current = cx
        .try_global::<Shortcuts>()
        .map(|s| s.0.clone())
        .unwrap_or_default();
    for (k, a) in &current {
        if a.as_deref() != Some(action) {
            continue;
        }
        // Un atajo por defecto se desactiva con `null`; uno propio se quita sin más.
        if DEFAULTS.iter().any(|(d, _)| d == k) {
            user.insert(k.clone(), None);
        } else {
            user.remove(k);
        }
    }
    let mut notice = None;
    if let Some(keys) = keys {
        if let Some(Some(other)) = current.get(keys)
            && other != action
        {
            notice = Some(format!(
                "{} ya no tiene atajo: {} pasó a «{}»",
                action_name(other),
                display(keys),
                action_name(action)
            ));
        }
        user.insert(keys.to_string(), Some(action.to_string()));
    }
    write_user(&user)?;
    let errors = load(cx);
    if !errors.is_empty() {
        return Err(errors.join(" · "));
    }
    Ok(notice)
}

/// Vuelve a los atajos por defecto.
pub fn reset(cx: &mut App) -> Vec<String> {
    let _ = fs::remove_file(keymap_path());
    ensure_keymap_file();
    load(cx)
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

    #[test]
    fn todas_las_acciones_del_panel_existen() {
        for (action, _) in ACTIONS {
            assert!(binding("ctrl-a", action).is_some(), "{action}");
        }
    }

    #[test]
    fn muestra_los_atajos_legibles() {
        assert_eq!(display("ctrl-shift-t"), "Ctrl+Shift+T");
        assert_eq!(display("ctrl-alt-w"), "Ctrl+Alt+W");
        assert_eq!(display("alt-up"), "Alt+↑");
        assert_eq!(display("f11"), "F11");
        assert_eq!(display("ctrl--"), "Ctrl+-");
        assert_eq!(display("ctrl-k ctrl-s"), "Ctrl+K Ctrl+S");
    }
}
