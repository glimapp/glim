use super::*;
use crate::views::settings_controls::{field_choice, field_dropdown};
use gpui_kit::assets::IconName;
use gpui_kit::component::WindowExt;

pub(super) fn navigation_button(
    id: impl Into<ElementId>,
    icon: IconName,
    label: &str,
    hint: &str,
) -> Button {
    Button::new(id)
        .ghost()
        .small()
        .h(px(46.))
        .px(px(3.))
        .tooltip(tr(hint))
        .accessibility_label(tr(hint))
        .child(
            v_flex()
                .items_center()
                .gap(px(3.))
                .child(gpui_kit::component::Icon::new(icon).size(px(14.)))
                .child(div().text_xs().child(tr(label))),
        )
}
pub(super) fn icon_action(
    button: Button,
    icon: impl Into<gpui_kit::component::Icon>,
    label: &str,
) -> Button {
    let label = tr(label);
    button
        .ghost()
        .small()
        .icon(icon.into())
        .tooltip(label.clone())
        .accessibility_label(label)
}

impl Changes {
    pub(super) fn branch_label(
        &self,
        branch: &str,
        ahead: usize,
        behind: usize,
    ) -> impl IntoElement {
        h_flex()
            .min_w_0()
            .gap_1()
            .px_2()
            .h(px(24.))
            .child(
                gpui_kit::component::Icon::new(IconName::GitBranch)
                    .size(px(14.))
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .child(branch.to_owned()),
            )
            .when(ahead > 0 || behind > 0, |row| {
                row.child(div().text_xs().child(format!("↑{ahead} ↓{behind}")))
            })
    }

    pub(super) fn fetch_button(&self, id: &'static str, cx: &Context<Self>) -> Button {
        icon_action(Button::new(id), IconName::Download, "Fetch")
            .disabled(self.active_root.is_none() || self.fetch_task.is_some())
            .loading(self.fetch_task.is_some())
            .on_click(cx.listener(|view, _, _, cx| view.fetch(cx)))
    }

    pub(super) fn compare_button(&self, id: &'static str, cx: &Context<Self>) -> Button {
        icon_action(
            Button::new(id),
            IconName::GitCompareArrows,
            "Compare revisions",
        )
        .disabled(self.active_root.is_none())
        .on_click(cx.listener(|view, _, window, cx| view.compare_dialog(window, cx)))
    }

    pub(super) fn feedback_key(
        &self,
        source: ElementId,
    ) -> Option<(PathBuf, Option<String>, ElementId)> {
        Some((
            self.active_root.clone()?,
            self.remote.as_ref().map(|s| s.connection.id.clone()),
            source,
        ))
    }
    pub(super) fn feedback_button(&self, source: ElementId, button: Button) -> Button {
        if self.manual_refresh.as_ref() == Some(&source) && self.loading {
            return button.loading(true);
        }
        match self
            .feedback_key(source)
            .and_then(|key| self.feedback.get(&key))
        {
            Some(None) => button.loading(true),
            Some(Some(_)) => button.icon(IconName::Check),
            None => button,
        }
    }
    pub(super) fn show_completed(
        &mut self,
        key: (PathBuf, Option<String>, ElementId),
        cx: &mut Context<Self>,
    ) {
        let finished = key.clone();
        let timer = cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1200))
                .await;
            let _ = view.update(cx, |view, cx| {
                view.feedback.remove(&finished);
                cx.notify();
            });
        });
        self.feedback.insert(key, Some(timer));
    }

    fn fetch(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.active_root.clone() else {
            return;
        };
        if self.fetch_task.is_some() {
            return;
        }
        self.operation_error = None;
        let remote = self.remote.clone();
        let connection = remote.as_ref().map(|session| session.connection.id.clone());
        let query_root = root.clone();
        let work = cx.background_executor().spawn(async move {
            git::fetch_remote_updates(&query_root, remote.as_ref(), &Cancellation::default())
        });
        self.fetch_task = Some(cx.spawn(async move |view, cx| {
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                view.fetch_task = None;
                if view.active_root.as_ref() == Some(&root)
                    && view.remote.as_ref().map(|session| &session.connection.id)
                        == connection.as_ref()
                {
                    match result {
                        Ok(_) => view.refresh(cx),
                        Err(error) => view.operation_error = Some(format!("{error:#}").into()),
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn error_dialog(
        &self,
        error: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.open_dialog(cx, move |dialog, _, _| {
            let error = error.clone();
            let copied = error.clone();
            dialog
                .title(tr("Git operation failed"))
                .width(px(640.))
                .child(
                    div()
                        .id("git-operation-details")
                        .max_h(px(360.))
                        .overflow_y_scroll()
                        .text_sm()
                        .child(error),
                )
                .child(
                    Button::new("git-copy-error")
                        .small()
                        .label(tr("Copy error details"))
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copied.to_string()))
                        }),
                )
        });
    }

    pub(super) fn preferences(cx: &App) -> glim_services::preferences::GitPreferences {
        cx.try_global::<crate::app::GitSettings>()
            .map(|settings| settings.preferences)
            .unwrap_or_default()
    }

    pub(super) fn settings_view(&self, cx: &Context<Self>) -> AnyElement {
        use gpui_kit::component::switch::Switch;
        let label = tr("Enable worktrees");
        v_flex()
            .id("git-settings-panel")
            .min_h_0()
            .p_2()
            .gap_2()
            .overflow_y_scroll()
            .child(
                h_flex()
                    .w_full()
                    .min_h(px(30.))
                    .gap_2()
                    .child(div().flex_1().text_sm().child(label.clone()))
                    .child(
                        Switch::new("git-worktrees")
                            .small()
                            .checked(Self::preferences(cx).worktrees)
                            .accessibility_label(label)
                            .on_click(cx.listener(|view, enabled, _, cx| {
                                view.save_preferences(
                                    glim_services::preferences::GitPreferences {
                                        worktrees: *enabled,
                                    },
                                    cx,
                                );
                            })),
                    ),
            )
            .into_any_element()
    }

    fn save_preferences(
        &self,
        preferences: glim_services::preferences::GitPreferences,
        cx: &mut Context<Self>,
    ) {
        let settings = cx.default_global::<crate::app::GitSettings>();
        settings.preferences = preferences;
        let previous = settings.save.take();
        let owner = cx.entity().downgrade();
        let task = cx.spawn(async move |_, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            if let Err(error) = cx
                .background_executor()
                .spawn(async move { glim_services::preferences::save_git_preferences(preferences) })
                .await
            {
                let _ = owner.update(cx, |view, cx| {
                    view.operation_error = Some(error.to_string().into());
                    cx.notify();
                });
            }
        });
        cx.global_mut::<crate::app::GitSettings>().save = Some(task);
    }

    fn compare_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.active_root.clone();
        let owner = cx.entity().downgrade();
        let initial = self.catalog.default_branch.as_deref().unwrap_or("HEAD");
        let fields = ["Base revision", "Target revision"].map(|label| {
            let input = cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state.set_value(initial, window, cx);
                state
            });
            (label, input)
        });
        let revisions = Arc::new(
            std::iter::once(("HEAD".into(), "HEAD".into()))
                .chain(
                    self.catalog
                        .references
                        .iter()
                        .map(|reference| (reference.name.clone(), reference.full.clone())),
                )
                .chain(self.commits.iter().map(|commit| {
                    (
                        format!(
                            "{}  {}",
                            &commit.id[..8.min(commit.id.len())],
                            commit.subject
                        ),
                        commit.id.clone(),
                    )
                }))
                .collect::<Vec<_>>(),
        );
        window.open_dialog(cx, move |dialog, window, cx| {
            let owner = owner.clone();
            let root = root.clone();
            let fields = fields.clone();
            let title = tr("Compare revisions");
            let mut dialog = dialog.title(title.clone()).width(px(480.));
            for (index, (label, input)) in fields.iter().enumerate() {
                let current = input.read(cx).value().to_string();
                let display = revisions
                    .iter()
                    .find(|(_, value)| value == &current)
                    .map_or_else(|| current.clone(), |(name, _)| name.clone());
                let revisions = revisions.clone();
                let input = input.clone();
                dialog = dialog.child(
                    v_flex()
                        .gap_1()
                        .child(div().text_sm().child(tr(label)))
                        .child(field_dropdown(
                            Button::new(("revision-picker", index))
                                .w_full()
                                .justify_start()
                                .accessibility_label(format!("{} · {display}", tr(label)))
                                .child(
                                    h_flex()
                                        .w_full()
                                        .min_w_0()
                                        .gap_2()
                                        .child(div().min_w_0().flex_1().truncate().child(display))
                                        .child(
                                            gpui_kit::component::Icon::new(IconName::ChevronDown)
                                                .size(px(14.)),
                                        ),
                                ),
                            window,
                            cx,
                            move |mut menu| {
                                for (name, value) in revisions.iter() {
                                    let selected = value == &current;
                                    let value = value.clone();
                                    let input = input.clone();
                                    menu = menu.item(
                                        field_choice(name.clone()).checked(selected).on_click(
                                            move |_, window, cx| {
                                                input.update(cx, |input, cx| {
                                                    input.set_value(value.clone(), window, cx)
                                                });
                                                window.refresh();
                                            },
                                        ),
                                    );
                                }
                                menu.scrollable(true).max_h(px(320.))
                            },
                        )),
                );
            }
            dialog
                .footer(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("git-form-cancel")
                                .label(tr("Cancel"))
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            crate::views::primary_button("git-form-submit", &title, false, cx)
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(
                                        Box::new(gpui_kit::component::dialog::Confirm {
                                            secondary: false,
                                        }),
                                        cx,
                                    )
                                }),
                        ),
                )
                .button_props(
                    gpui_kit::component::dialog::DialogButtonProps::default()
                        .ok_text(tr("Compare revisions"))
                        .cancel_text(tr("Cancel")),
                )
                .on_ok(move |_, _, cx| {
                    let left = fields[0].1.read(cx).value().trim().to_owned();
                    let right = fields[1].1.read(cx).value().trim().to_owned();
                    let _ = owner.update(cx, |view, cx| {
                        if view.active_root == root {
                            view.open_comparison(Comparison::Between(left, right), cx);
                        }
                    });
                    true
                })
        });
    }
}
