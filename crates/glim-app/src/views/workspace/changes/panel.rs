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
        let disabled =
            self.operation_busy || crate::app::workspace_editors_locked(cx) || self.loading;
        let metadata = metadata.unwrap_or_default();
        let header = h_flex()
            .id("git-repository-toolbar")
            .overflow_x_scroll()
            .w_full()
            .min_w_0()
            .h(px(38.))
            .flex_shrink_0()
            .px_2()
            .gap_2()
            .child(div().flex_1().min_w_0().truncate().text_sm().child(format!(
                    "{} · {}",
                    self.active_root
                        .as_ref()
                        .map(|p| p
                            .file_name()
                            .unwrap_or(p.as_os_str())
                            .to_string_lossy()
                            .into_owned())
                        .unwrap_or_else(|| tr("No Git repository")),
                    if metadata.branch == "(detached)" {
                        tr("Detached HEAD")
                    } else {
                        metadata.branch.clone()
                    }
                )))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{}  ↑{} ↓{}",
                        metadata.upstream, metadata.ahead, metadata.behind
                    )),
            )
            .child(self.repository_menu(cx))
            .child(self.operation_button("git-fetch", Operation::Fetch(None), disabled, cx))
            .child(self.operation_menu(
                "git-pull",
                "Pull",
                if metadata.upstream.is_empty() {
                    vec![]
                } else {
                    vec![Operation::Pull(false), Operation::Pull(true)]
                },
                cx,
            ))
            .child(self.operation_button(
                "git-push",
                Operation::Push(None),
                disabled || metadata.upstream.is_empty(),
                cx,
            ))
            .child(
                Button::new("git-refresh-repository")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::RefreshCw)
                    .tooltip(tr("Refresh"))
                    .accessibility_label(tr("Refresh"))
                    .disabled(self.loading || self.operation_busy)
                    .on_click(cx.listener(|view, _, _, cx| view.refresh(cx))),
            );
        let mut tabs = h_flex()
            .id("git-sections")
            .overflow_x_scroll()
            .min_w_0()
            .w_full()
            .flex_shrink_0()
            .px_2()
            .gap_1();
        for section in Section::ALL {
            tabs = tabs.child(
                Button::new(("git-section", section as usize))
                    .ghost()
                    .small()
                    .flex_shrink_0()
                    .label(tr(section.label()))
                    .selected(self.section == section)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.switch_section(section, cx);
                        cx.emit(ShowDiff);
                    })),
            );
        }
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
        } else if let Some(label) = self.operation_label.clone() {
            status = status.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .truncate()
                    .child(label),
            );
        } else {
            status = status.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        self.active_root
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                    ),
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
        if self.section == Section::Changes {
            let mut actions = h_flex()
                .px_2()
                .py_1()
                .gap_1()
                .flex_wrap()
                .child(
                    self.operation_button(
                        "git-stage-all",
                        Operation::Stage(None),
                        disabled || working == 0 || conflict_count > 0,
                        cx,
                    )
                    .label(tr("Stage all")),
                )
                .child(
                    self.operation_button(
                        "git-unstage-all",
                        Operation::Unstage(None),
                        disabled || staged == 0,
                        cx,
                    )
                    .label(tr("Unstage all")),
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
                    actions = actions.child(
                        self.operation_button(
                            "git-stage-file",
                            Operation::Stage(Some(change.clone())),
                            disabled,
                            cx,
                        )
                        .label(tr(if change.status == 'U' {
                            "Mark resolved"
                        } else {
                            "Stage file"
                        })),
                    );
                }
                if change.area == Area::Untracked {
                    actions = actions.child(self.operation_button(
                        "git-ignore-file",
                        Operation::Ignore(change.path.clone()),
                        disabled,
                        cx,
                    ));
                }
                if change.area == Area::Working && change.status != 'U' {
                    actions = actions.child(self.operation_button(
                        "git-discard-file",
                        Operation::Discard(change.clone()),
                        disabled,
                        cx,
                    ));
                }
                if change.status == 'M'
                    && change.previous.is_none()
                    && change.area != Area::Untracked
                {
                    let selected = self
                        .diff
                        .read(cx)
                        .selected_lines()
                        .is_some_and(|range| range.side() == range.end_side());
                    actions = actions.child(
                        Button::new("git-stage-blocks")
                            .small()
                            .label(tr(if change.area == Area::Staged {
                                "Unstage selected blocks"
                            } else {
                                "Stage selected blocks"
                            }))
                            .disabled(disabled || !selected || self.patch_source.is_none())
                            .on_click(
                                cx.listener(|view, _, window, cx| view.selected_blocks(window, cx)),
                            ),
                    );
                }
                if change.status == 'U' {
                    actions = actions.child(self.operation_menu(
                        "git-conflict-sides",
                        "Resolve file",
                        vec![
                            Operation::ConflictSide {
                                path: change.path.clone(),
                                incoming: false,
                            },
                            Operation::ConflictSide {
                                path: change.path.clone(),
                                incoming: true,
                            },
                        ],
                        cx,
                    ));
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
                        Button::new("git-save-resolution")
                            .small()
                            .label(tr("Save resolution and stage"))
                            .disabled(disabled || !resolved || self.patch_source.is_none())
                            .on_click(
                                cx.listener(|view, _, window, cx| view.save_resolution(window, cx)),
                            ),
                    );
                }
            }
            content = content
                .child(actions)
                .child(div().flex_1().min_h_0().w_full().child(self.diff_view(cx)));
            let message = self.message.read(cx).value().to_string();
            content = content.child(
                v_flex()
                    .flex_shrink_0()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .child(
                        Textarea::new(&self.message)
                            .h(px(78.))
                            .aria_label(tr("Commit message"))
                            .disabled(disabled),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("{} {}", staged, tr("staged files"))),
                            )
                            .child(
                                Button::new("git-amend-toggle")
                                    .small()
                                    .label(tr("Amend"))
                                    .selected(self.amend)
                                    .disabled(disabled || metadata.head.is_empty())
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.amend = !view.amend;
                                        cx.notify();
                                    })),
                            )
                            .child(self.operation_button(
                                "git-commit",
                                Operation::Commit {
                                    message,
                                    amend: self.amend,
                                },
                                disabled
                                    || conflict_count > 0
                                    || (staged == 0 && !self.amend)
                                    || self.message.read(cx).value().to_string().trim().is_empty(),
                                cx,
                            )),
                    ),
            );
        } else {
            content = content.child(self.browser_view(window, cx));
            if matches!(
                self.section,
                Section::History | Section::Branches | Section::Stashes
            ) {
                content =
                    content.child(div().flex_1().min_h_0().w_full().child(self.diff_view(cx)));
            }
        }
        v_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(header)
            .child(tabs)
            .child(status)
            .child(content)
            .into_any_element()
    }
}
