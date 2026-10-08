//! Panel de ajustes (`Ctrl+,`): tema, fuentes, tamaño de letra, opciones y atajos.
//! Cada cambio se informa como un [`Change`]; quien abre el panel lo aplica y lo guarda.

use std::rc::Rc;

use gpui::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::{IndexPath, Selectable as _};
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    searchable_list::SearchableVec,
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
    v_flex,
};

use crate::{keymap, settings::Settings, theme};

type Choices = SearchableVec<SharedString>;

/// Primera opción de las listas de fuentes: volver a la del sistema.
const EDITOR_DEFAULT: &str = "Predeterminada (Consolas)";
const UI_DEFAULT: &str = "Predeterminada (Segoe UI)";

/// Lo que el usuario cambió en el panel.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Theme(SharedString),
    Dark(bool),
    /// `None` vuelve a la fuente por defecto.
    EditorFont(Option<SharedString>),
    UiFont(Option<SharedString>),
    FontSize(f32),
    AlwaysOnTop(bool),
    Autosave(bool),
    StartInPreview(bool),
    /// Empezar (o dejar) de grabar el atajo de una acción.
    Record(&'static str),
    ResetShortcuts,
    EditFile,
    OpenThemesFolder,
    Close,
}

pub type OnChange = Rc<dyn Fn(Change, &mut Window, &mut App)>;

pub struct SettingsPanel {
    pub focus_handle: FocusHandle,
    theme: Entity<SelectState<Choices>>,
    editor_font: Entity<SelectState<Choices>>,
    ui_font: Entity<SelectState<Choices>>,
    /// Acción cuyo atajo se está grabando: la próxima tecla pulsada lo reemplaza.
    pub recording: Option<&'static str>,
    /// Escucha las teclas mientras el panel está abierto, para grabar atajos.
    pub key_recorder: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}

/// Crea una lista desplegable con `items`, con `selected` elegido si está en ella.
fn select<T: 'static>(
    items: Vec<SharedString>,
    selected: &str,
    searchable: bool,
    window: &mut Window,
    cx: &mut Context<T>,
) -> Entity<SelectState<Choices>> {
    let ix = items
        .iter()
        .position(|i| i.as_ref() == selected)
        .unwrap_or(0);
    cx.new(|cx| {
        SelectState::new(Choices::new(items), Some(IndexPath::new(ix)), window, cx)
            .searchable(searchable)
    })
}

impl SettingsPanel {
    pub fn new<T: 'static>(
        settings: &Settings,
        on_change: impl Fn(&mut T, Change, &mut Window, &mut Context<T>) + 'static,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Self {
        let fonts = theme::fonts(cx);
        let with_default = |default: &str| {
            let mut items = vec![SharedString::from(default.to_string())];
            items.extend(fonts.iter().cloned());
            items
        };

        let theme = select(theme::names(cx), &settings.theme, false, window, cx);
        let editor_font = select(
            with_default(EDITOR_DEFAULT),
            settings.font_family.as_deref().unwrap_or(EDITOR_DEFAULT),
            true,
            window,
            cx,
        );
        let ui_font = select(
            with_default(UI_DEFAULT),
            settings.ui_font_family.as_deref().unwrap_or(UI_DEFAULT),
            true,
            window,
            cx,
        );

        let on_change = Rc::new(on_change);
        let mut subscribe = |state: &Entity<SelectState<Choices>>,
                             to_change: fn(SharedString) -> Change| {
            let on_change = on_change.clone();
            cx.subscribe_in(
                state,
                window,
                move |this, _, ev: &SelectEvent<Choices>, window, cx| {
                    let SelectEvent::Confirm(Some(value)) = ev else {
                        return;
                    };
                    on_change(this, to_change(value.clone()), window, cx);
                },
            )
        };
        let subscriptions = vec![
            subscribe(&theme, Change::Theme),
            subscribe(&editor_font, |v| {
                Change::EditorFont((v.as_ref() != EDITOR_DEFAULT).then_some(v))
            }),
            subscribe(&ui_font, |v| {
                Change::UiFont((v.as_ref() != UI_DEFAULT).then_some(v))
            }),
        ];

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            focus_handle,
            theme,
            editor_font,
            ui_font,
            recording: None,
            key_recorder: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn render(&self, settings: &Settings, on_change: &OnChange, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;

        let row = |label: &'static str, hint: Option<String>, control: AnyElement| {
            h_flex()
                .gap_4()
                .py_2()
                .items_center()
                .justify_between()
                .child(
                    v_flex()
                        .min_w_0()
                        .child(div().text_sm().child(label))
                        .children(hint.map(|h| div().text_xs().text_color(muted).child(h))),
                )
                .child(control)
        };
        let section = |title: &'static str| {
            div()
                .pt_3()
                .pb_1()
                .text_xs()
                .font_semibold()
                .text_color(muted)
                .child(title.to_uppercase())
        };
        let send = |change: Change| {
            let on_change = on_change.clone();
            move |window: &mut Window, cx: &mut App| on_change(change.clone(), window, cx)
        };
        let switch = |id: &'static str, checked: bool, change: fn(bool) -> Change| {
            let on_change = on_change.clone();
            Switch::new(id)
                .checked(checked)
                .on_click(move |v, window, cx| on_change(change(*v), window, cx))
                .into_any_element()
        };
        let dropdown = |state: &Entity<SelectState<Choices>>, search: &'static str| {
            div()
                .w(px(210.))
                .flex_shrink_0()
                .child(
                    Select::new(state)
                        .small()
                        .search_placeholder(search)
                        .menu_max_h(px(280.)),
                )
                .into_any_element()
        };

        let size = settings.font_size;
        let font_size = h_flex()
            .gap_1()
            .items_center()
            .child({
                let f = send(Change::FontSize(size - 1.0));
                Button::new("font-smaller")
                    .ghost()
                    .small()
                    .icon(IconName::Minus)
                    .tooltip(keymap::label("ZoomOut", cx).unwrap_or_default())
                    .on_click(move |_, window, cx| f(window, cx))
            })
            .child(
                div()
                    .w(px(44.))
                    .text_center()
                    .text_sm()
                    .child(format!("{size:.0} px")),
            )
            .child({
                let f = send(Change::FontSize(size + 1.0));
                Button::new("font-bigger")
                    .ghost()
                    .small()
                    .icon(IconName::Plus)
                    .tooltip(keymap::label("ZoomIn", cx).unwrap_or_default())
                    .on_click(move |_, window, cx| f(window, cx))
            })
            .into_any_element();

        let shortcut = |action: &'static str| {
            let recording = self.recording == Some(action);
            let keys = keymap::keys_for(action, cx);
            let text = if recording {
                "Pulsa el atajo…".to_string()
            } else if keys.is_empty() {
                "Sin asignar".to_string()
            } else {
                keys.iter()
                    .map(|k| keymap::display(k))
                    .collect::<Vec<_>>()
                    .join("  ·  ")
            };
            let record = send(Change::Record(action));
            Button::new(ElementId::Name(format!("key-{action}").into()))
                .outline()
                .small()
                .selected(recording)
                .label(text)
                .on_click(move |_, window, cx| record(window, cx))
                .into_any_element()
        };
        let joined = |actions: &[&str]| {
            let keys: Vec<_> = actions
                .iter()
                .filter_map(|a| keymap::label(a, cx))
                .collect();
            (!keys.is_empty()).then(|| keys.join(" / "))
        };

        let close = send(Change::Close);
        let reset_shortcuts = send(Change::ResetShortcuts);
        let edit_file = send(Change::EditFile);
        let open_themes = send(Change::OpenThemesFolder);

        v_flex()
            .w_full()
            .child(
                h_flex()
                    .px_4()
                    .py_2()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(div().text_sm().font_semibold().child("Ajustes"))
                    .child(
                        Button::new("settings-close")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .tooltip("Cerrar (Esc)")
                            .on_click(move |_, window, cx| close(window, cx)),
                    ),
            )
            .child(
                v_flex()
                    .id("settings-body")
                    .px_4()
                    .pb_3()
                    .max_h(px(440.))
                    .overflow_y_scroll()
                    .child(section("Apariencia"))
                    .child(row("Tema", None, dropdown(&self.theme, "Buscar tema…")))
                    .child(row(
                        "Oscuro",
                        keymap::label("ToggleTheme", cx),
                        switch("dark", settings.dark, Change::Dark),
                    ))
                    .child(section("Texto"))
                    .child(row(
                        "Fuente del editor",
                        Some("Modo edición".into()),
                        dropdown(&self.editor_font, "Buscar fuente…"),
                    ))
                    .child(row(
                        "Fuente de la interfaz",
                        Some("Vista Markdown y menús".into()),
                        dropdown(&self.ui_font, "Buscar fuente…"),
                    ))
                    .child(row(
                        "Tamaño de letra",
                        joined(&["ZoomIn", "ZoomOut"]),
                        font_size,
                    ))
                    .child(section("Comportamiento"))
                    .child(row(
                        "Guardar automáticamente",
                        Some("Al dejar de escribir".into()),
                        switch("autosave", settings.autosave, Change::Autosave),
                    ))
                    .child(row(
                        "Siempre encima",
                        keymap::label("TogglePin", cx),
                        switch("on-top", settings.always_on_top, Change::AlwaysOnTop),
                    ))
                    .child(row(
                        "Abrir en modo vista",
                        Some("Al iniciar la app".into()),
                        switch(
                            "start-preview",
                            settings.start_in_preview,
                            Change::StartInPreview,
                        ),
                    ))
                    .child(section("Atajos"))
                    .children(
                        keymap::ACTIONS
                            .iter()
                            .map(|&(action, name)| row(name, None, shortcut(action))),
                    )
                    .child(row(
                        "Restablecer atajos",
                        Some(
                            "Clic en un atajo para cambiarlo · Esc cancela · Supr lo quita".into(),
                        ),
                        Button::new("reset-shortcuts")
                            .ghost()
                            .small()
                            .label("Restablecer")
                            .on_click(move |_, window, cx| reset_shortcuts(window, cx))
                            .into_any_element(),
                    )),
            )
            .child(
                h_flex()
                    .px_4()
                    .py_2()
                    .gap_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        Button::new("edit-settings")
                            .ghost()
                            .small()
                            .label("Editar settings.json")
                            .on_click(move |_, window, cx| edit_file(window, cx)),
                    )
                    .child(
                        Button::new("themes-folder")
                            .ghost()
                            .small()
                            .icon(IconName::FolderOpen)
                            .label("Carpeta de temas")
                            .on_click(move |_, window, cx| open_themes(window, cx)),
                    ),
            )
    }
}
