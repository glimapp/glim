use super::*;
use crate::views::primary_button;
use gpui_kit::component::tooltip::Tooltip;

fn connection_dot(id: impl Into<ElementId>, connected: bool, cx: &App) -> AnyElement {
    let status = tr(if connected {
        "Connected"
    } else {
        "Not connected"
    });
    div()
        .id(id)
        .flex_shrink_0()
        .size(px(16.))
        .flex()
        .items_center()
        .justify_center()
        .role(Role::Status)
        .aria_label(status.clone())
        .tooltip(move |window, cx| Tooltip::new(status.clone()).build(window, cx))
        .child(
            div()
                .size(px(6.))
                .rounded_full()
                .bg(crate::views::connection_color(connected, cx)),
        )
        .into_any_element()
}

fn connection_input(
    state: &Entity<InputState>,
    label: &str,
    disabled: bool,
    window: &Window,
    cx: &App,
) -> Input {
    crate::views::form_input(state, window, cx)
        .aria_label(label.to_owned())
        .disabled(disabled)
        .when(disabled, |input| input.border_color(cx.theme().input))
}

impl RemoteDialog {
    pub(super) fn title(&self) -> String {
        tr(match self.mode {
            Mode::Connections => "SSH Connections",
            Mode::Edit if self.connection.id.is_empty() => "Add SSH Connection",
            Mode::Edit => "Edit SSH Connection",
            Mode::Directories => "Open Remote Folder",
        })
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::Directories && self.blocked() {
            self.dismiss_feedback(window, cx);
            return;
        }
        self.task = None;
        self.tested_session = None;
        self.busy = false;
        if (self.mode == Mode::Edit && !self.open_after_save)
            || (self.mode == Mode::Directories && !self.add_folder)
        {
            self.mode = Mode::Connections;
            self.message = None;
            self.message_success = false;
            self.feedback_task = None;
            window.focus(&self.modal_focus, cx);
            cx.notify();
        } else {
            self.close(window, cx);
        }
    }

    fn dismiss_feedback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.task = None;
        self.feedback_task = None;
        self.busy = false;
        self.message = None;
        self.message_success = false;
        window.focus(&self.modal_focus, cx);
        cx.notify();
    }

    fn blocked(&self) -> bool {
        self.busy || self.message.is_some()
    }

    fn feedback_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("ssh-feedback-overlay")
            .absolute()
            .inset_0()
            .rounded(cx.theme().radius_lg)
            .bg(cx.theme().background.opacity(0.94))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .flex()
            .items_center()
            .justify_center()
            .p_5()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(440.))
                    .max_h_full()
                    .gap_4()
                    .items_center()
                    .child(self.feedback(cx))
                    .when(!self.message_success, |body| {
                        body.child(
                            Button::new("ssh-dismiss-feedback")
                                .outline()
                                .when(self.busy, |button| button.label(tr("Cancel")))
                                .when(!self.busy, |button| {
                                    button
                                        .icon(Icon::new(IconName::X).size(px(16.)))
                                        .tooltip(tr("Close"))
                                        .accessibility_label(tr("Close"))
                                })
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.dismiss_feedback(window, cx)
                                })),
                        )
                    }),
            )
            .into_any_element()
    }

    fn feedback(&self, cx: &App) -> AnyElement {
        if self.busy {
            return h_flex()
                .min_h(px(32.))
                .gap_2()
                .child(Spinner::new().small())
                .child(div().text_sm().child(tr(self.activity)))
                .into_any_element();
        }
        let Some(message) = &self.message else {
            return div().into_any_element();
        };
        let color = if self.message_success {
            crate::views::accent(cx)
        } else {
            cx.theme()
                .danger
                .blend(
                    cx.theme()
                        .foreground
                        .opacity(if cx.theme().is_dark() { 0. } else { 0.22 }),
                )
        };
        h_flex()
            .id("ssh-feedback")
            .gap_2()
            .items_start()
            .py_1()
            .child(
                Icon::new(if self.message_success {
                    IconName::CircleCheck
                } else {
                    IconName::CircleAlert
                })
                .size(px(16.))
                .mt(px(2.))
                .text_color(color)
                .flex_shrink_0(),
            )
            .child(
                div()
                    .id("ssh-feedback-text")
                    .flex_1()
                    .min_w_0()
                    .max_h(px(180.))
                    .overflow_y_scroll()
                    .text_sm()
                    .text_color(color)
                    .whitespace_normal()
                    .child(message.clone()),
            )
            .into_any_element()
    }

    fn field(&self, index: usize, label: &'static str, window: &Window, cx: &App) -> AnyElement {
        v_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(div().text_sm().child(tr(label)))
            .child(connection_input(
                &self.fields[index],
                &tr(label),
                self.blocked(),
                window,
                cx,
            ))
            .into_any_element()
    }

    fn render_connections(&self, cx: &mut Context<Self>) -> AnyElement {
        let connections = cx.global::<Connections>().catalog.connections.clone();
        let mut list = v_flex()
            .id("ssh-connections-list")
            .w_full()
            .min_h(px(64.))
            .max_h(px(320.))
            .track_scroll(&self.connections_scroll)
            .overflow_y_scroll();
        if connections.is_empty() {
            list = list.child(
                h_flex()
                    .py_5()
                    .gap_2()
                    .text_color(cx.theme().muted_foreground)
                    .child(Icon::new(IconName::Globe).size(px(18.)))
                    .child(div().text_sm().child(tr("No SSH connections yet"))),
            );
        }
        for (index, connection) in connections.into_iter().enumerate() {
            let owner = cx.entity().downgrade();
            let edit = connection.clone();
            let delete = connection.clone();
            let label = connection.label();
            let open = connection.clone();
            let connected = crate::app::remote::connection_is_connected(&connection.id, cx);
            list = list.child(
                h_flex()
                    .id(("ssh-connection", index))
                    .w_full()
                    .min_w_0()
                    .gap_3()
                    .py_3()
                    .when(index > 0, |row| {
                        row.border_t_1().border_color(cx.theme().border)
                    })
                    .child(connection_dot(("ssh-status", index), connected, cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(label.clone()),
                            )
                            .when(!connection.name.trim().is_empty(), |text| {
                                text.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .truncate()
                                        .child(connection.address()),
                                )
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("ssh-open-{}", connection.id)))
                            .disabled(self.blocked())
                            .small()
                            .outline()
                            .label(tr("Browse…"))
                            .accessibility_label(format!("{} · {label}", tr("Browse…")))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.connect(open.clone(), window, cx)
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("ssh-options-{}", connection.id)))
                            .disabled(self.blocked())
                            .small()
                            .outline()
                            .icon(IconName::Ellipsis)
                            .tooltip(tr("Connection Options"))
                            .accessibility_label(format!("{} · {label}", tr("Connection Options")))
                            .dropdown_menu(move |menu, _, _| {
                                let edit = edit.clone();
                                let delete = delete.clone();
                                let editor = owner.clone();
                                let remover = owner.clone();
                                menu.item(PopupMenuItem::new(tr("Edit…")).on_click(
                                    move |_, window, cx| {
                                        let _ = editor.update(cx, |view, cx| {
                                            view.open_after_save = false;
                                            view.edit(edit.clone(), window, cx);
                                        });
                                    },
                                ))
                                .separator()
                                .item(
                                    PopupMenuItem::new(tr("Delete Connection…")).on_click(
                                        move |_, window, cx| {
                                            let _ = remover.update(cx, |view, cx| {
                                                view.delete(delete.clone(), window, cx)
                                            });
                                        },
                                    ),
                                )
                            }),
                    ),
            );
        }
        v_flex()
            .w_full()
            .gap_4()
            .child(list)
            .child(
                h_flex()
                    .w_full()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .justify_between()
                    .child(
                        primary_button("ssh-add", &tr("Add Connection…"), self.blocked(), cx)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.open_after_save = false;
                                view.edit(Connection::default(), window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    fn render_form(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .w_full()
            .gap_5()
            .child(self.field(0, "Connection Name", window, cx))
            .child(
                h_flex()
                    .w_full()
                    .gap_3()
                    .items_start()
                    .child(self.field(1, "SSH Host", window, cx))
                    .child(
                        div()
                            .w(px(100.))
                            .flex_shrink_0()
                            .child(self.field(3, "Port", window, cx)),
                    ),
            )
            .child(self.field(2, "Username", window, cx))
            .child(
                v_flex()
                    .gap_2()
                    .child(div().text_sm().child(tr("Private Key")))
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .child(
                                div().flex_1().min_w_0().child(
                                    connection_input(
                                        &self.fields[4],
                                        &tr("Private Key"),
                                        self.blocked(),
                                        window,
                                        cx,
                                    )
                                    .cleanable(true),
                                ),
                            )
                            .child(
                                Button::new("ssh-key-file")
                                    .outline()
                                    .label(tr("Choose…"))
                                    .disabled(self.blocked())
                                    .on_click(
                                        cx.listener(|view, _, window, cx| {
                                            view.pick_key(window, cx)
                                        }),
                                    ),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_3()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("ssh-test")
                            .outline()
                            .label(tr("Test Connection"))
                            .disabled(self.blocked())
                            .on_click(
                                cx.listener(|view, _, window, cx| view.save(true, window, cx)),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("ssh-cancel")
                            .disabled(self.blocked())
                            .outline()
                            .label(tr("Cancel"))
                            .on_click(cx.listener(|view, _, window, cx| view.cancel(window, cx))),
                    )
                    .child(
                        primary_button(
                            "ssh-save",
                            &tr(if self.open_after_save {
                                "Save and Connect"
                            } else {
                                "Save"
                            }),
                            self.blocked(),
                            cx,
                        )
                        .on_click(cx.listener(|view, _, window, cx| view.save(false, window, cx))),
                    ),
            )
            .into_any_element()
    }

    fn render_directories(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let connections = cx.global::<Connections>().catalog.connections.clone();
        let owner = cx.entity().downgrade();
        let same_connection = self.add_folder.then(|| self.connection.id.clone());
        let selector = Button::new("ssh-location")
            .small()
            .outline()
            .text_color(cx.theme().muted_foreground)
            .disabled(self.blocked() || self.add_folder)
            .accessibility_label(tr("SSH Connection"))
            .child(
                h_flex()
                    .min_w_0()
                    .gap_2()
                    .child(connection_dot(
                        "ssh-current-status",
                        self.session.as_ref().is_some_and(Session::is_connected),
                        cx,
                    ))
                    .child(
                        div()
                            .truncate()
                            .max_w(px(260.))
                            .child(self.connection.label()),
                    )
                    .child(Icon::new(IconName::ChevronDown).size(px(14.))),
            )
            .dropdown_menu(move |mut menu, _, _| {
                for connection in &connections {
                    if same_connection
                        .as_ref()
                        .is_some_and(|id| *id != connection.id)
                    {
                        continue;
                    }
                    let connection = connection.clone();
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(connection.label()).on_click(
                        move |_, window, cx| {
                            let _ = owner.update(cx, |view, cx| {
                                view.connect(connection.clone(), window, cx)
                            });
                        },
                    ));
                }
                menu
            });
        let entries = self
            .entries
            .iter()
            .filter(|entry| entry.is_dir)
            .cloned()
            .collect::<Vec<_>>();
        let count = entries.len();
        let focused = self.focus.clone();
        let list = div()
            .id("ssh-folder-list")
            .relative()
            .h(px(256.))
            .flex_shrink_0()
            .w_full()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(6.))
            .track_focus(&focused)
            .tab_index(0)
            .focus_visible(|style| style.border_color(crate::views::accent(cx)))
            .overflow_hidden()
            .when(count == 0 && !self.busy, |list| {
                list.child(
                    v_flex()
                        .size_full()
                        .justify_center()
                        .items_center()
                        .gap_2()
                        .child(
                            Icon::new(IconName::Folder)
                                .size(px(24.))
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(tr(if self.message.is_some() {
                                    "Folders unavailable"
                                } else {
                                    "No subfolders"
                                })),
                        ),
                )
            })
            .when(count > 0, |list| {
                list.child(
                    uniform_list(
                        "ssh-directories",
                        count,
                        cx.processor(move |view, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| {
                                    let path = entries[index].path.clone();
                                    let selected = view.selected.as_ref() == Some(&path);
                                    h_flex()
                                        .id(("ssh-directory", index))
                                        .h(px(34.))
                                        .w_full()
                                        .px_3()
                                        .gap_2()
                                        .items_center()
                                        .when(selected, |row| {
                                            row.bg(crate::views::selection_background(cx))
                                        })
                                        .when(!selected, |row| {
                                            row.hover(|row| row.bg(cx.theme().muted))
                                        })
                                        .child(
                                            Icon::new(IconName::Folder).size(px(16.)).text_color(
                                                if selected {
                                                    crate::views::accent(cx)
                                                } else {
                                                    cx.theme().muted_foreground
                                                },
                                            ),
                                        )
                                        .child(
                                            div().flex_1().min_w_0().truncate().text_sm().child(
                                                path.file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy()
                                                    .into_owned(),
                                            ),
                                        )
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                move |view, event: &MouseDownEvent, window, cx| {
                                                    if view.blocked() {
                                                        return;
                                                    }
                                                    window.focus(&view.focus, cx);
                                                    if event.click_count == 2 {
                                                        view.browse(path.clone(), window, cx);
                                                    } else {
                                                        view.selected = Some(path.clone());
                                                        cx.notify();
                                                    }
                                                },
                                            ),
                                        )
                                        .into_any_element()
                                })
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .size_full(),
                )
            })
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                view.directory_key(event, window, cx)
            }));
        v_flex()
            .w_full()
            .gap_3()
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .justify_between()
                    .gap_3()
                    .child(selector)
                    .child(
                        Button::new("ssh-refresh")
                            .small()
                            .outline()
                            .icon(IconName::RotateCw)
                            .loading(self.busy && self.activity == "Loading folders…")
                            .tooltip(tr("Refresh"))
                            .accessibility_label(tr("Refresh"))
                            .disabled(self.blocked())
                            .on_click(cx.listener(|view, _, window, cx| {
                                if view.session.is_none() {
                                    view.connect(view.connection.clone(), window, cx);
                                } else {
                                    view.browse(view.directory.clone(), window, cx);
                                }
                            })),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .child(
                        Button::new("ssh-parent")
                            .outline()
                            .icon(IconName::ArrowUp)
                            .tooltip(tr("Parent Folder"))
                            .accessibility_label(tr("Parent Folder"))
                            .disabled(self.blocked() || self.directory.parent().is_none())
                            .on_click(cx.listener(|view, _, window, cx| {
                                if let Some(parent) = view.directory.parent() {
                                    view.browse(parent.to_path_buf(), window, cx);
                                }
                            })),
                    )
                    .child(div().flex_1().min_w_0().child(connection_input(
                        &self.path,
                        &tr("Remote folder path"),
                        self.blocked() || self.session.is_none(),
                        window,
                        cx,
                    )))
                    .child(
                        Button::new("ssh-go")
                            .outline()
                            .icon(IconName::ArrowRight)
                            .tooltip(tr("Go to Folder"))
                            .accessibility_label(tr("Go to Folder"))
                            .disabled(
                                self.blocked()
                                    || self.session.is_none()
                                    || self.path.read(cx).value().trim().is_empty(),
                            )
                            .on_click(cx.listener(|view, _, window, cx| {
                                let path = PathBuf::from(view.path.read(cx).value().trim());
                                view.browse(path, window, cx);
                            })),
                    ),
            )
            .child(list)
            .child(
                h_flex()
                    .w_full()
                    .gap_3()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        v_flex().flex_1().min_w_0().gap_1().child(
                            div()
                                .id("ssh-target-path")
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .tooltip({
                                    let path = self.target_directory(cx).display().to_string();
                                    move |window, cx| Tooltip::new(path.clone()).build(window, cx)
                                })
                                .child(self.target_directory(cx).display().to_string()),
                        ),
                    )
                    .child(
                        Button::new("ssh-cancel")
                            .outline()
                            .label(tr("Cancel"))
                            .on_click(cx.listener(|view, _, window, cx| view.cancel(window, cx))),
                    )
                    .child(
                        primary_button(
                            "ssh-open-folder",
                            &tr("Open Folder"),
                            self.blocked()
                                || self.session.is_none()
                                || self.path.read(cx).value().trim().is_empty(),
                            cx,
                        )
                        .on_click(cx.listener(|view, _, window, cx| view.open(window, cx))),
                    ),
            )
            .into_any_element()
    }

    fn directory_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.blocked() {
            return;
        }
        let paths = self
            .entries
            .iter()
            .filter(|entry| entry.is_dir)
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let current = self
            .selected
            .as_ref()
            .and_then(|selected| paths.iter().position(|path| path == selected));
        let next = match event.keystroke.key.as_str() {
            "down" => {
                Some(current.map_or(0, |index| (index + 1).min(paths.len().saturating_sub(1))))
            }
            "up" => Some(current.unwrap_or(0).saturating_sub(1)),
            "enter" | "right" => {
                if let Some(path) = self.selected.clone() {
                    self.browse(path, window, cx);
                }
                cx.stop_propagation();
                None
            }
            "left" => {
                if let Some(parent) = self.directory.parent() {
                    self.browse(parent.to_path_buf(), window, cx);
                }
                cx.stop_propagation();
                None
            }
            _ => None,
        };
        if let Some(index) = next
            && let Some(path) = paths.get(index)
        {
            self.selected = Some(path.clone());
            self.scroll.scroll_to_item(index, ScrollStrategy::Top);
            cx.stop_propagation();
            cx.notify();
        }
    }
}

impl Render for RemoteDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.mode {
            Mode::Connections => self.render_connections(cx),
            Mode::Edit => self.render_form(window, cx),
            Mode::Directories => self.render_directories(window, cx),
        };
        let dialog = cx.entity().downgrade();
        gpui_kit::base::Dialog::new(cx)
            .focus_handle(self.modal_focus.clone())
            .close_on_backdrop_press(false)
            .on_ok(|_, _, _| false)
            .request_close(move |_, window, cx| {
                let _ = dialog.update(cx, |view, cx| view.cancel(window, cx));
            })
            .backdrop(div().size_full().bg(cx.theme().overlay))
            .popup(
                v_flex()
                    .id("ssh-dialog")
                    .relative()
                    .w(px(560.))
                    .max_w((window.viewport_size().width - px(32.)).max(px(0.)))
                    .max_h((window.viewport_size().height - px(32.)).max(px(0.)))
                    .p_5()
                    .gap_5()
                    .rounded(cx.theme().radius_lg)
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
                    .shadow_lg()
                    .occlude()
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .child(
                                gpui_kit::base::DialogTitle::new()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.title()),
                            )
                            .when(self.mode == Mode::Connections, |header| {
                                header.child(
                                    crate::views::icon_button(
                                        "ssh-close",
                                        IconName::X,
                                        tr("Close"),
                                    )
                                    .ghost()
                                    .disabled(self.blocked())
                                    .on_click(
                                        cx.listener(|view, _, window, cx| view.cancel(window, cx)),
                                    ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id(match self.mode {
                                Mode::Connections => "ssh-manager-body",
                                Mode::Edit => "ssh-form-body",
                                Mode::Directories => "ssh-browser-body",
                            })
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(body),
                    )
                    .when(
                        self.blocked()
                            && !(self.mode == Mode::Directories
                                && self.busy
                                && self.activity == "Loading folders…"),
                        |surface| surface.child(self.feedback_overlay(cx)),
                    ),
            )
    }
}
