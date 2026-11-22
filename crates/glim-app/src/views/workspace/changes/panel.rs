use super::browser::Section;
use super::*;
use gpui_kit::component::input::Textarea;

impl Render for Changes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !git::enabled() {
            return message(
                tr(if git::AVAILABLE {
                    "Git is disabled"
                } else {
                    "Git is unavailable in this build"
                }),
                cx,
            );
        }
        if self.message_root != self.active_root {
            if let Some(root) = self.message_root.take() {
                self.drafts.insert(
                    root,
                    (self.message.read(cx).value().to_string(), self.amend),
                );
            }
            let (message, amend) = self
                .active_root
                .as_ref()
                .and_then(|root| self.drafts.get(root))
                .cloned()
                .unwrap_or_default();
            self.message
                .update(cx, |input, cx| input.set_value(message, window, cx));
            self.amend = amend;
            self.message_root = self.active_root.clone();
        }
        if let Some(message) = self.clear_message.take() {
            if self.message.read(cx).value().to_string() == message {
                self.message
                    .update(cx, |input, cx| input.set_value("", window, cx));
            }
        }
        let repository = self
            .active_root
            .as_ref()
            .and_then(|root| self.repositories.iter().find(|repo| &repo.root == root));
        let metadata = repository.map(|repo| repo.metadata.clone());
        let staged = repository.map_or(0, |repo| {
            repo.changes
                .iter()
                .filter(|c| c.area == Area::Staged)
                .count()
        });
        let working = repository.map_or(0, |repo| {
            repo.changes
                .iter()
                .filter(|c| c.area != Area::Staged)
                .count()
        });
        let conflict_count = repository.map_or(0, |repo| {
            repo.changes.iter().filter(|c| c.status == 'U').count()
        });
        let disabled = self.loading && self.repositories.is_empty();
        let metadata = metadata.unwrap_or_default();
        let branch = if metadata.branch == "(detached)" {
            tr("Detached HEAD")
        } else if metadata.branch.is_empty() {
            tr("No Git repository")
        } else {
            metadata.branch.clone()
        };
        let header = h_flex()
            .id("git-repository-toolbar")
            .w_full()
            .min_w_0()
            .flex_wrap()
            .py_1()
            .flex_shrink_0()
            .px_2()
            .gap_1()
            .child(self.branch_picker(
                &branch,
                repository.map(|r| r.branches.clone()).unwrap_or_default(),
                cx,
            ))
            .child(self.subsection_button(
                Section::Stashes,
                gpui_kit::assets::IconName::Archive,
                cx,
            ))
            .when(metadata.ahead > 0 || metadata.behind > 0, |row| {
                row.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("↑{} ↓{}", metadata.ahead, metadata.behind)),
                )
            })
            .child(self.operation_button("git-fetch", Operation::Fetch(None), disabled, cx))
            .when(!metadata.upstream.is_empty(), |row| {
                row.child(self.operation_button(
                    "git-pull",
                    Operation::Pull(Self::preferences(cx).pull_rebase),
                    disabled,
                    cx,
                ))
                .child(self.operation_button(
                    "git-push",
                    Operation::Push(None),
                    disabled,
                    cx,
                ))
            });
        use gpui_kit::assets::IconName;
        let mut tabs = h_flex()
            .id("git-sections")
            .w_full()
            .min_w_0()
            .overflow_x_scroll()
            .flex_shrink_0()
            .justify_around()
            .px_1();
        for section in Section::PRIMARY {
            if section == Section::Worktrees && !Self::preferences(cx).worktrees {
                continue;
            }
            let (icon, label) = match section {
                Section::Changes => (IconName::Files, "Changes"),
                Section::History => (IconName::Clock, "History"),
                Section::Branches => (IconName::GitBranch, "Git branches"),
                Section::Remotes => (IconName::Server, "Remotes"),
                Section::Stashes => (IconName::Archive, "Stashes"),
                Section::Worktrees => (IconName::Folders, "Worktree tab"),
            };
            let active = !self.settings_open && self.section.parent() == section;
            tabs = tabs.child(
                div()
                    .flex_shrink_0()
                    .border_b_2()
                    .border_color(if active {
                        crate::views::accent(cx)
                    } else {
                        transparent_black()
                    })
                    .child(
                        super::controls::navigation_button(
                            ("git-section", section as usize),
                            icon,
                            label,
                            section.label(),
                        )
                        .toggled(active)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.settings_open = false;
                            view.switch_section(section, cx);
                            cx.notify();
                            cx.emit(ShowDiff);
                        })),
                    ),
            );
        }
        let tabs = tabs.child(
            div()
                .flex_shrink_0()
                .border_b_2()
                .border_color(if self.settings_open {
                    crate::views::accent(cx)
                } else {
                    transparent_black()
                })
                .child(
                    super::controls::navigation_button(
                        "git-settings",
                        IconName::Settings,
                        "Settings",
                        "Settings",
                    )
                    .toggled(self.settings_open)
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.settings_open = true;
                        view.load_identity(window, cx);
                        cx.notify();
                    })),
                ),
        );
        let mut status = h_flex().h(px(30.)).flex_shrink_0().min_w_0().px_3().gap_2();
        if let Some(error) = self
            .operation_error
            .clone()
            .or_else(|| self.browser_error.clone())
        {
            status = status.child(
                Button::new("git-operation-error")
                    .ghost()
                    .small()
                    .label(tr(if self.operation_error.is_some() {
                        "Git operation failed"
                    } else {
                        "Git query failed"
                    }))
                    .text_color(cx.theme().danger)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.error_dialog(error.clone(), window, cx)
                    })),
            );
        }
        if let Some(operation) = metadata.operation.clone() {
            status = status
                .child(div().text_xs().child(format!(
                    "{} · {} {}",
                    operation,
                    conflict_count,
                    tr("Conflicts")
                )))
                .child(self.operation_button(
                    "git-continue",
                    Operation::Continue(operation.clone()),
                    disabled || conflict_count > 0,
                    cx,
                ))
                .child(self.operation_button(
                    "git-abort",
                    Operation::Abort(operation),
                    disabled,
                    cx,
                ));
        }
        let mut content = v_flex().flex_1().min_h_0().w_full();
        if self.settings_open {
            content = content.child(self.settings_view(window, cx));
        } else if self.section == Section::Changes {
            let mut actions = header.child(
                super::controls::icon_action(
                    Button::new("all-changes-diff"),
                    gpui_kit::assets::IconName::Layers,
                    "All changes",
                )
                .disabled(self.active_root.is_none())
                .on_click(cx.listener(|view, _, _, cx| {
                    view.selection = None;
                    view.show_comparison(Comparison::AllChanges, cx);
                    cx.emit(ShowDiff);
                })),
            );
            if let Some((_, change)) = self
                .selection
                .clone()
                .filter(|(root, _)| Some(root) == self.active_root.as_ref())
            {
                if change.area == Area::Staged {
                    actions = actions.child(self.operation_button(
                        "git-unstage-file",
                        Operation::Unstage(Some(change.clone())),
                        disabled,
                        cx,
                    ));
                } else {
                    actions = actions.child(self.operation_button(
                        "git-stage-file",
                        Operation::Stage(Some(change.clone())),
                        disabled,
                        cx,
                    ));
                }
                if change.status == 'M'
                    && change.previous.is_none()
                    && change.area != Area::Untracked
                    && self
                        .diff
                        .read(cx)
                        .selected_lines()
                        .is_some_and(|range| range.side() == range.end_side())
                {
                    actions = actions.child(
                        self.feedback_button(
                            "git-stage-blocks".into(),
                            Button::new("git-stage-blocks"),
                        )
                        .small()
                        .label(tr(if change.area == Area::Staged {
                            "Unstage selected blocks"
                        } else {
                            "Stage selected blocks"
                        }))
                        .disabled(disabled || self.patch_source.is_none())
                        .on_click(
                            cx.listener(|view, _, window, cx| view.selected_blocks(window, cx)),
                        ),
                    );
                }
                if change.status == 'U' {
                    for incoming in [false, true] {
                        actions = actions.child(self.operation_button(
                            ("git-conflict-side", usize::from(incoming)),
                            Operation::ConflictSide {
                                path: change.path.clone(),
                                incoming,
                            },
                            disabled,
                            cx,
                        ));
                    }
                    let resolved = self.diff.read(cx).files().first().is_some_and(|file| {
                        !file.conflicts().is_empty()
                            && file.conflicts().iter().enumerate().all(|(index, _)| {
                                self.diff
                                    .read(cx)
                                    .conflict_resolution(file.path(), index)
                                    .is_some()
                            })
                    });
                    actions = actions.child(
                        self.feedback_button(
                            "git-save-resolution".into(),
                            Button::new("git-save-resolution"),
                        )
                        .small()
                        .label(tr("Save resolution and stage"))
                        .disabled(disabled || !resolved || self.patch_source.is_none())
                        .on_click(
                            cx.listener(|view, _, window, cx| view.save_resolution(window, cx)),
                        ),
                    );
                }
            }
            let mut batch = Vec::new();
            if working > 0 && conflict_count == 0 {
                batch.push(Operation::Stage(None));
            }
            if staged > 0 {
                batch.push(Operation::Unstage(None));
            }
            for (index, op) in batch.into_iter().enumerate() {
                actions =
                    actions.child(self.operation_button(("git-batch", index), op, disabled, cx));
            }
            actions = actions.child(self.operation_button(
                "git-submodules",
                Operation::Submodules,
                disabled,
                cx,
            ));
            content = content.child(actions).child(self.navigation(cx));
            let message = self.message.read(cx).value().to_string();
            let focused = self.message.read(cx).focus_handle(cx).is_focused(window);
            content = content.child(
                div().p_2().child(
                    v_flex()
                        .p_1()
                        .rounded_md()
                        .border_1()
                        .bg(cx.theme().background)
                        .border_color(if focused {
                            crate::views::accent(cx)
                        } else {
                            cx.theme().input
                        })
                        .child(
                            Textarea::new(&self.message)
                                .appearance(false)
                                .bordered(false)
                                .h(px(54.))
                                .aria_label(tr("Commit message"))
                                .disabled(disabled),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .gap_1()
                                .child(
                                    div()
                                        .flex_1()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .when(staged > 0, |v| {
                                            v.child(format!("{} {}", staged, tr("staged files")))
                                        }),
                                )
                                .child(
                                    super::controls::icon_action(
                                        Button::new("git-amend-toggle"),
                                        IconName::Undo2,
                                        "Amend",
                                    )
                                    .selected(self.amend)
                                    .disabled(disabled || metadata.head.is_empty())
                                    .on_click(cx.listener(
                                        |view, _, _, cx| {
                                            view.amend = !view.amend;
                                            cx.notify();
                                        },
                                    )),
                                )
                                .child(
                                    self.feedback_button(
                                        "git-commit-refresh".into(),
                                        super::controls::icon_action(
                                            Button::new("git-commit-refresh"),
                                            IconName::RefreshCw,
                                            "Refresh",
                                        ),
                                    )
                                    .disabled(self.loading)
                                    .on_click(cx.listener(
                                        |view, _, _, cx| {
                                            view.refresh_from("git-commit-refresh", cx)
                                        },
                                    )),
                                )
                                .child(
                                    self.operation_button(
                                        "git-commit",
                                        Operation::Commit {
                                            message,
                                            amend: self.amend,
                                        },
                                        disabled
                                            || conflict_count > 0
                                            || (staged == 0 && !self.amend)
                                            || self
                                                .message
                                                .read(cx)
                                                .value()
                                                .to_string()
                                                .trim()
                                                .is_empty(),
                                        cx,
                                    )
                                    .primary(),
                                ),
                        ),
                ),
            );
        } else {
            content = content.child(self.browser_view(window, cx));
        }
        v_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(
                v_flex()
                    .w_full()
                    .px_3()
                    .flex_shrink_0()
                    .child(tabs)
                    .child(div().w_full().h(px(1.)).bg(cx.theme().border)),
            )
            .when(
                self.operation_error.is_some()
                    || self.browser_error.is_some()
                    || metadata.operation.is_some(),
                |panel| panel.child(status),
            )
            .child(content)
            .into_any_element()
    }
}
