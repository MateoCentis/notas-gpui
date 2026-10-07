//! Dibujo de la ventana: barra de título, editor o vista, barra de estado y paneles.

use std::rc::Rc;

use gpui::{
    App, ClickEvent, Context, Div, ExternalPaths, InteractiveElement as _, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, ParentElement as _, Render, Stateful,
    StatefulInteractiveElement as _, Styled as _, Window, div, img, prelude::FluentBuilder as _,
    px, relative,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::Selectable as _;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, StyledExt as _, TITLE_BAR_HEIGHT, TitleBar,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::Editor,
    text::TextView,
    v_flex,
};

use crate::{
    keymap::{
        self, CloseSettings, OpenSettings, SearchNotes, TogglePin, TogglePreview, ToggleTheme,
    },
    settings_panel::OnChange,
    tasks::Block,
};

use super::{Doc, LOGO, Notas};

impl Notas {
    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (title, dirty) = match self.active() {
            Some(d) => (d.title.clone(), d.dirty),
            None => ("Notas".into(), false),
        };
        let open = self.docs.len();

        let icon_button = |id: &'static str, icon: IconName, tooltip: &'static str| {
            Button::new(id).ghost().xsmall().icon(icon).tooltip(tooltip)
        };

        TitleBar::new()
            .bg(theme.title_bar)
            .border_color(theme.title_bar_border)
            .child(
                h_flex()
                    .min_w_0()
                    .gap_2()
                    .items_center()
                    .child(img(LOGO).size(px(16.)).flex_shrink_0())
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .font_medium()
                            .text_color(theme.foreground)
                            .child(title),
                    )
                    .when(dirty, |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .size(px(6.))
                                .rounded_full()
                                .bg(theme.primary),
                        )
                    })
                    .when(open > 1, |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .px_1p5()
                                .rounded_md()
                                .bg(theme.secondary)
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("{open} abiertas")),
                        )
                    }),
            )
            .child(
                h_flex()
                    .id("title-actions")
                    // Tapa la zona de arrastre de la barra: sin esto Windows trata el
                    // clic como "mover ventana" y los botones no reciben el clic.
                    .occlude()
                    .flex_shrink_0()
                    .gap_0p5()
                    .pr_1()
                    .child(
                        icon_button("search", IconName::Search, "Buscar notas (Ctrl+K)").on_click(
                            cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.search_notes(&SearchNotes, window, cx)
                            }),
                        ),
                    )
                    .child(
                        icon_button(
                            "preview",
                            if self.preview {
                                IconName::Pencil
                            } else {
                                IconName::Eye
                            },
                            if self.preview {
                                "Editar (Ctrl+E)"
                            } else {
                                "Vista (Ctrl+E)"
                            },
                        )
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, window, cx| {
                                this.toggle_preview(&TogglePreview, window, cx)
                            },
                        )),
                    )
                    .child(
                        icon_button(
                            "pin",
                            if self.settings.always_on_top {
                                IconName::PinOff
                            } else {
                                IconName::Pin
                            },
                            "Siempre encima (Ctrl+Shift+T)",
                        )
                        .selected(self.settings.always_on_top)
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, window, cx| {
                                this.toggle_pin(&TogglePin, window, cx)
                            },
                        )),
                    )
                    .child(
                        icon_button("settings", IconName::Settings, "Ajustes (Ctrl+,)")
                            .selected(self.settings_panel.is_some())
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.open_settings(&OpenSettings, window, cx)
                            })),
                    )
                    .child(
                        icon_button(
                            "theme",
                            if self.settings.dark {
                                IconName::Sun
                            } else {
                                IconName::Moon
                            },
                            "Tema claro/oscuro (Ctrl+Shift+D)",
                        )
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, window, cx| {
                                this.toggle_theme(&ToggleTheme, window, cx)
                            },
                        )),
                    ),
            )
    }

    fn render_preview(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let blocks = self.blocks(cx);
        let id = self.active().map_or(0, |d| d.id);
        let muted = cx.theme().muted_foreground;
        let entity = cx.entity().downgrade();

        let content = if blocks.is_empty() {
            v_flex().child(
                div()
                    .text_color(muted)
                    .child("Nota vacía. Pulsa Ctrl+E para escribir."),
            )
        } else {
            v_flex()
                .gap_1()
                .children(blocks.iter().enumerate().map(|(i, block)| {
                    match block {
                        Block::Markdown(md) => div()
                            .py_1()
                            .child(TextView::markdown(("md", i), md.clone()).selectable(true))
                            .into_any_element(),
                        Block::Task(task) => {
                            let mark = task.mark.clone();
                            let entity = entity.clone();
                            h_flex()
                                .items_start()
                                .gap_2()
                                .pl(px(task.indent as f32 * 8.0))
                                .child(div().pt(px(3.)).child(
                                    Checkbox::new(("task", i)).checked(task.checked).on_click(
                                        move |checked, window, cx| {
                                            let mark = mark.clone();
                                            let _ = entity.update(cx, |this, cx| {
                                                this.set_mark(mark, *checked, window, cx)
                                            });
                                        },
                                    ),
                                ))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .when(task.checked, |d| d.text_color(muted).line_through())
                                        .child(TextView::markdown(
                                            ("task-text", i),
                                            task.text.clone(),
                                        )),
                                )
                                .into_any_element()
                        }
                    }
                }))
        };

        div()
            .id(("preview", id))
            .size_full()
            .overflow_y_scroll()
            .px_5()
            .py_3()
            .child(content)
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let hint = |keys: &'static str, text: &'static str| {
            h_flex()
                .gap_3()
                .items_center()
                .child(
                    div()
                        .w(px(64.))
                        .px_1p5()
                        .py_0p5()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.secondary)
                        .text_xs()
                        .text_center()
                        .child(keys),
                )
                .child(div().text_color(muted).child(text))
        };
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .child(img(LOGO).size(px(56.)).opacity(0.9))
            .child(div().text_color(muted).child("No hay ninguna nota abierta"))
            .child(
                v_flex()
                    .gap_2()
                    .text_sm()
                    .child(hint("Ctrl N", "Nueva nota"))
                    .child(hint("Ctrl K", "Buscar notas")),
            )
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let location = match self.active() {
            Some(Doc { path: Some(p), .. }) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            Some(_) => "Sin guardar".into(),
            None => String::new(),
        };
        let (done, total) = self.active().map_or((0, 0), |d| d.progress);

        h_flex()
            .h(px(26.))
            .px_3()
            .gap_3()
            .border_t_1()
            .border_color(theme.title_bar_border)
            .bg(theme.title_bar)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(div().truncate().child(location))
            .when_some(self.status.clone(), |this, status| {
                this.child(div().truncate().text_color(theme.primary).child(status))
            })
            .child(div().flex_1())
            .when(total > 0, |this| {
                let ratio = done as f32 / total as f32;
                this.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .w(px(56.))
                                .h(px(4.))
                                .rounded_full()
                                .bg(theme.secondary)
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(ratio))
                                        .rounded_full()
                                        .bg(theme.primary),
                                ),
                        )
                        .child(format!("{done}/{total} tareas")),
                )
            })
    }

    fn render_palette(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let palette = self.palette.as_ref()?;
        let weak = cx.entity().downgrade();
        let (q, c, x) = (weak.clone(), weak.clone(), weak.clone());

        let command = palette.render(
            move |query, _, cx| {
                let _ = q.update(cx, |this, cx| {
                    if let Some(p) = this.palette.as_mut() {
                        p.set_query(query);
                    }
                    cx.notify();
                });
            },
            move |ix, window, cx| {
                let _ = c.update(cx, |this, cx| this.confirm_palette(ix.row, window, cx));
            },
            move |window, cx| {
                let _ = x.update(cx, |this, cx| this.close_palette(window, cx));
            },
            cx,
        );

        Some(Self::render_modal(
            "palette-backdrop",
            cx.listener(|this, _, window, cx| this.close_palette(window, cx)),
            cx,
            Self::modal_panel("palette", cx)
                // Escape con la búsqueda vacía cierra; con texto, lo borra la paleta.
                .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    if ev.keystroke.key != "escape" || ev.keystroke.modifiers.modified() {
                        return;
                    }
                    let empty = this
                        .palette
                        .as_ref()
                        .is_none_or(|p| p.state.read(cx).query(cx).is_empty());
                    if empty {
                        cx.stop_propagation();
                        this.close_palette(window, cx);
                    }
                }))
                .child(command),
        ))
    }

    fn render_settings(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let panel = self.settings_panel.as_ref()?;
        let weak = cx.entity().downgrade();
        let on_change: OnChange = Rc::new(move |change, window, cx| {
            let _ = weak.update(cx, |this, cx| this.on_settings_change(change, window, cx));
        });

        Some(Self::render_modal(
            "settings-backdrop",
            cx.listener(|this, _, window, cx| this.close_settings(&CloseSettings, window, cx)),
            cx,
            Self::modal_panel("settings", cx)
                .key_context(keymap::SETTINGS_CONTEXT)
                .track_focus(&panel.focus_handle)
                .text_color(cx.theme().popover_foreground)
                // El tamaño de letra del editor no se aplica al panel.
                .text_size(px(14.))
                .child(panel.render(&self.settings, &on_change, cx)),
        ))
    }

    /// Caja de un panel flotante (buscador o ajustes).
    fn modal_panel(id: &'static str, cx: &App) -> Stateful<Div> {
        let theme = cx.theme();
        div()
            .id(id)
            .w_full()
            .max_w(px(460.))
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_lg()
            .overflow_hidden()
            // Los clics dentro del panel no deben cerrarlo.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }

    /// Fondo oscurecido bajo un panel flotante; un clic fuera del panel llama a `on_close`.
    fn render_modal(
        id: &'static str,
        on_close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &App,
        panel: impl IntoElement,
    ) -> impl IntoElement {
        div()
            .id(id)
            .absolute()
            // Debajo de la barra de título, para poder mover o cerrar la ventana.
            .top(TITLE_BAR_HEIGHT)
            .left_0()
            .right_0()
            .bottom_0()
            .occlude()
            .bg(cx.theme().overlay)
            .flex()
            .flex_col()
            .items_center()
            .pt(px(20.))
            .px_4()
            .on_mouse_down(MouseButton::Left, on_close)
            .child(panel)
    }
}

impl Render for Notas {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match self.active() {
            Some(d) => format!("{}{} — Notas", if d.dirty { "• " } else { "" }, d.title),
            None => "Notas".into(),
        };
        if title != self.last_title {
            window.set_window_title(&title);
            self.last_title = title;
        }

        let font_size = px(self.settings.font_size);
        let font_family = self.editor_font.clone();

        let body = match self.active() {
            None => self.render_empty(cx).into_any_element(),
            Some(_) if self.preview => self.render_preview(cx).into_any_element(),
            Some(doc) => div()
                .size_full()
                .pl_5()
                .pr_1()
                .pt_3()
                .pb_1()
                .child(
                    Editor::new(&doc.editor)
                        .appearance(false)
                        .h_full()
                        .text_size(font_size)
                        .when_some(font_family, |e, f| e.font_family(f)),
                )
                .into_any_element(),
        };

        v_flex()
            .relative()
            .key_context(keymap::CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::new_file))
            .on_action(cx.listener(Self::open_file))
            .on_action(cx.listener(Self::search_notes))
            .on_action(cx.listener(Self::close_note))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::toggle_preview))
            .on_action(cx.listener(Self::toggle_pin))
            .on_action(cx.listener(Self::toggle_maximize))
            .on_action(cx.listener(Self::toggle_task))
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
            .on_action(cx.listener(Self::zoom_reset))
            .on_action(cx.listener(Self::open_keymap))
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(Self::close_settings))
            .on_action(cx.listener(Self::quit))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                for path in paths.paths() {
                    this.open_path(path.clone(), window, cx);
                }
            }))
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_title_bar(cx))
            .child(div().flex_1().min_h_0().text_size(font_size).child(body))
            .child(self.render_status_bar(cx))
            .children(self.render_palette(cx))
            .children(self.render_settings(cx))
    }
}
