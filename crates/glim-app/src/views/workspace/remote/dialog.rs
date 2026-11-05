use super::*;
use gpui_kit::component::button::ButtonCustomVariant;

fn primary_button(id: &'static str, label: &str, cx: &App) -> Button {
    let ink = crate::views::accent(cx);
    Button::new(id).label(label.to_owned()).custom(
        ButtonCustomVariant::new(cx)
            .color(ink)
            .foreground(cx.theme().background)
            .hover(ink.blend(cx.theme().foreground.opacity(0.08)))
            .active(ink.blend(cx.theme().foreground.opacity(0.16))),
    )
}

impl RemoteDialog {
    pub(super) fn title(&self) -> String {
        tr(match self.mode {
            Mode::Connections => "SSH Connections",
            Mode::Edit if self.connection.id.is_empty() => "Add SSH Connection…",
            Mode::Edit => "Edit SSH Connection",
            Mode::Directories => "Open Remote Folder",
        })
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.task = None;
        self.busy = false;
        if self.mode == Mode::Edit && !self.open_after_save {
            self.mode = Mode::Connections;
            self.message = None;
            cx.notify();
        } else {
            window.close_dialog(cx);
        }
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
            .p_3()
            .rounded(px(6.))
            .bg(cx.theme().background.blend(color.opacity(0.08)))
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
                    .max_h(px(100.))
                    .overflow_y_scroll()
                    .text_sm()
                    .text_color(color)
                    .whitespace_normal()
                    .child(message.clone()),
            )
            .into_any_element()
    }

    fn field(&self, index: usize, label: &'static str) -> AnyElement {
        v_flex()
            .flex_1()
            .min_w_0()
            .gap_1()
            .child(div().text_sm().child(tr(label)))
            .child(
                Input::new(&self.fields[index])
                    .aria_label(tr(label))
                    .disabled(self.busy),
            )
            .into_any_element()
    }

    fn render_connections(&self, cx: &mut Context<Self>) -> AnyElement {
        let connections = cx.global::<Connections>().catalog.connections.clone();
        let mut list = v_flex()
            .id("ssh-connections-list")
            .w_full()
            .max_h(px(320.))
            .overflow_y_scroll();
        if connections.is_empty() {
            list = list.child(
                v_flex()
                    .py_8()
                    .gap_2()
                    .items_center()
                    .child(
                        Icon::new(IconName::Globe)
                            .size(px(24.))
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(div().text_sm().child(tr("No SSH connections yet")))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr("Add a connection to browse remote folders.")),
                    ),
            );
        }
        for (index, connection) in connections.into_iter().enumerate() {
            let owner = cx.entity().downgrade();
            let edit = connection.clone();
            let delete = connection.clone();
            let label = connection.label();
            let open = connection.clone();
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
                    .child(
                        Icon::new(IconName::Globe)
                            .size(px(18.))
                            .text_color(cx.theme().muted_foreground)
                            .flex_shrink_0(),
                    )
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
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr("Browse remote folders and open files read-only.")),
            )
            .child(list)
            .child(self.feedback(cx))
            .child(
                h_flex()
                    .w_full()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .justify_between()
                    .child(
                        primary_button("ssh-add", &tr("Add Connection…"), cx).on_click(
                            cx.listener(|view, _, window, cx| {
                                view.open_after_save = false;
                                view.edit(Connection::default(), window, cx);
                            }),
                        ),
                    )
                    .child(
                        Button::new("ssh-done")
                            .outline()
                            .label(tr("Done"))
                            .on_click(cx.listener(|view, _, window, cx| view.cancel(window, cx))),
                    ),
            )
            .into_any_element()
    }

    fn render_form(&self, cx: &mut Context<Self>) -> AnyElement {
        let owner = cx.entity().downgrade();
        let authentication = Button::new("ssh-authentication")
            .outline()
            .disabled(self.busy)
            .accessibility_label(tr("Authentication"))
            .child(
                h_flex()
                    .gap_2()
                    .child(tr(if self.explicit_key {
                        "Private Key"
                    } else {
                        "System SSH"
                    }))
                    .child(Icon::new(IconName::ChevronDown).size(px(14.))),
            )
            .dropdown_menu(move |menu, _, _| {
                let system = owner.clone();
                let key = owner.clone();
                menu.item(
                    PopupMenuItem::new(tr("System SSH")).on_click(move |_, _, cx| {
                        let _ = system.update(cx, |view, cx| {
                            view.explicit_key = false;
                            view.message = None;
                            cx.notify();
                        });
                    }),
                )
                .item(
                    PopupMenuItem::new(tr("Private Key")).on_click(move |_, _, cx| {
                        let _ = key.update(cx, |view, cx| {
                            view.explicit_key = true;
                            view.message = None;
                            cx.notify();
                        });
                    }),
                )
            });
        v_flex().w_full().gap_4()
            .child(h_flex().w_full().gap_3().items_start()
                .child(self.field(1, "SSH Host"))
                .child(div().w(px(112.)).flex_shrink_0().child(self.field(3, "Port"))))
            .child(h_flex().w_full().gap_3().items_start()
                .child(self.field(2, "Username"))
                .child(self.field(0, "Connection Name")))
            .child(v_flex().gap_2().pt_1()
                .child(h_flex().w_full().justify_between().child(div().text_sm().child(tr("Authentication"))).child(authentication))
                .when(!self.explicit_key, |body| body.child(div().text_sm().text_color(cx.theme().muted_foreground).whitespace_normal()
                    .child(tr("Uses your SSH config, agent and local keys. Passwords are requested when needed."))))
                .when(self.explicit_key, |body| body.child(h_flex().w_full().gap_2()
                    .child(div().flex_1().min_w_0().child(Input::new(&self.fields[4]).aria_label(tr("Private Key")).disabled(self.busy)))
                    .child(Button::new("ssh-key-file").outline().label(tr("Choose…")).disabled(self.busy)
                        .on_click(cx.listener(|view, _, window, cx| view.pick_key(window, cx)))))))
            .child(self.feedback(cx))
            .child(h_flex().w_full().gap_3().pt_3().border_t_1().border_color(cx.theme().border)
                .child(Button::new("ssh-test").outline().label(tr("Test Connection")).disabled(self.busy)
                    .loading(self.busy && self.activity == "Testing connection…")
                    .on_click(cx.listener(|view, _, window, cx| view.save(true, window, cx))))
                .child(div().flex_1())
                .child(Button::new("ssh-cancel").outline().label(tr("Cancel"))
                    .on_click(cx.listener(|view, _, window, cx| view.cancel(window, cx))))
                .child(primary_button("ssh-save", &tr(if self.open_after_save { "Save and Connect" } else { "Save" }), cx)
                    .disabled(self.busy).loading(self.busy && self.activity == "Saving connection…")
                    .on_click(cx.listener(|view, _, window, cx| view.save(false, window, cx)))))
            .into_any_element()
    }

    fn render_directories(&self, cx: &mut Context<Self>) -> AnyElement {
        let connections = cx.global::<Connections>().catalog.connections.clone();
        let owner = cx.entity().downgrade();
        let same_connection = self.add_folder.then(|| self.connection.id.clone());
        let selector = Button::new("ssh-location")
            .outline()
            .disabled(self.busy || self.add_folder)
            .accessibility_label(tr("SSH Connection"))
            .child(
                h_flex()
                    .min_w_0()
                    .gap_2()
                    .child(Icon::new(IconName::Globe).size(px(16.)))
                    .child(
                        div()
                            .truncate()
                            .max_w(px(300.))
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
            .filter(|entry| {
                entry.is_dir
                    && (self.show_hidden
                        || !entry
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .starts_with('.'))
            })
            .cloned()
            .collect::<Vec<_>>();
        let count = entries.len();
        let focused = self.focus.clone();
        let list = div()
            .id("ssh-folder-list")
            .h(px(256.))
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
                                        .when(selected, |row| {
                                            row.child(
                                                Icon::new(IconName::Check)
                                                    .size(px(14.))
                                                    .text_color(crate::views::accent(cx)),
                                            )
                                        })
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                move |view, event: &MouseDownEvent, window, cx| {
                                                    if view.busy {
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
                    .when(self.session.is_some() && !self.busy, |row| {
                        row.child(
                            h_flex()
                                .gap_1()
                                .flex_shrink_0()
                                .text_sm()
                                .text_color(crate::views::accent(cx))
                                .child(Icon::new(IconName::CircleCheck).size(px(14.)))
                                .child(tr("Connected")),
                        )
                    })
                    .child(
                        Button::new("ssh-refresh")
                            .outline()
                            .icon(IconName::RotateCw)
                            .tooltip(tr("Refresh"))
                            .accessibility_label(tr("Refresh"))
                            .disabled(self.busy)
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
                            .disabled(self.busy || self.directory.parent().is_none())
                            .on_click(cx.listener(|view, _, window, cx| {
                                if let Some(parent) = view.directory.parent() {
                                    view.browse(parent.to_path_buf(), window, cx);
                                }
                            })),
                    )
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.path)
                                .aria_label(tr("Remote folder path"))
                                .disabled(self.busy || self.session.is_none()),
                        ),
                    )
                    .child(
                        Button::new("ssh-go")
                            .outline()
                            .icon(IconName::ArrowRight)
                            .tooltip(tr("Go to Folder"))
                            .accessibility_label(tr("Go to Folder"))
                            .disabled(
                                self.busy
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
                Checkbox::new("ssh-hidden")
                    .small()
                    .label(tr("Show hidden folders"))
                    .checked(self.show_hidden)
                    .disabled(self.busy)
                    .on_click(cx.listener(|view, checked: &bool, _, cx| {
                        view.show_hidden = *checked;
                        view.selected = None;
                        cx.notify();
                    })),
            )
            .child(self.feedback(cx))
            .child(
                v_flex()
                    .w_full()
                    .gap_3()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        v_flex()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(tr("Folder to open")),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .truncate()
                                    .child(self.target_directory(cx).display().to_string()),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("ssh-cancel")
                                    .outline()
                                    .label(tr("Cancel"))
                                    .on_click(
                                        cx.listener(|view, _, window, cx| view.cancel(window, cx)),
                                    ),
                            )
                            .child(
                                primary_button("ssh-open-folder", &tr("Open Folder"), cx)
                                    .disabled(
                                        self.busy
                                            || self.session.is_none()
                                            || self.path.read(cx).value().trim().is_empty(),
                                    )
                                    .loading(self.busy && self.activity == "Opening folder…")
                                    .on_click(
                                        cx.listener(|view, _, window, cx| view.open(window, cx)),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn directory_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let paths = self
            .entries
            .iter()
            .filter(|entry| {
                entry.is_dir
                    && (self.show_hidden
                        || !entry
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .starts_with('.'))
            })
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.mode {
            Mode::Connections => self.render_connections(cx),
            Mode::Edit => self.render_form(cx),
            Mode::Directories => self.render_directories(cx),
        }
    }
}
