use super::browser::Section;
use super::*;

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
        let repository = self
            .active_root
            .as_ref()
            .and_then(|root| self.repositories.iter().find(|repo| &repo.root == root));
        let metadata = repository.map(|repo| repo.metadata.clone());
        let conflict_count = repository.map_or(0, |repo| {
            repo.changes.iter().filter(|c| c.status == 'U').count()
        });
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
            .child(self.branch_label(&branch, metadata.ahead, metadata.behind))
            .child(self.fetch_button("git-fetch", cx))
            .child(self.subsection_button(
                Section::Stashes,
                gpui_kit::assets::IconName::Archive,
                cx,
            ));
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
                Section::Stashes => (IconName::Archive, "Stashes"),
                Section::Worktrees => (IconName::Network, "Worktree tab"),
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
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.settings_open = true;
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
            status = status.child(div().text_xs().child(format!(
                "{} · {} {}",
                operation,
                conflict_count,
                tr("Conflicts")
            )));
        }
        let mut content = v_flex().flex_1().min_h_0().w_full();
        if self.settings_open {
            content = content.child(self.settings_view(cx));
        } else if self.section == Section::Changes {
            let actions = header.child(
                super::controls::icon_action(
                    Button::new("all-changes-diff"),
                    gpui_kit::assets::IconName::Layers,
                    "All changes",
                )
                .disabled(self.active_root.is_none())
                .on_click(cx.listener(|view, _, _, cx| {
                    view.selection = None;
                    view.open_comparison(Comparison::AllChanges, cx);
                })),
            );
            content = content.child(actions).child(self.navigation(cx));
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
