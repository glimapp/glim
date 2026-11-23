use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    WindowExt,
    menu::{DropdownMenu, PopupMenuItem},
};

pub(super) fn icon_action(button: Button, icon: IconName, label: &'static str) -> Button {
    let label = tr(label);
    button
        .small()
        .ghost()
        .icon(icon)
        .tooltip(label.clone())
        .accessibility_label(label)
}

impl Changes {
    pub(super) fn active_change_count(&self) -> usize {
        self.rows.len()
    }

    pub(super) fn repository_header(&self, cx: &Context<Self>) -> impl IntoElement {
        let root = self.active_root.clone();
        let repository = self
            .repositories
            .iter()
            .find(|repo| Some(&repo.root) == root.as_ref());
        let branch = repository
            .map(|repo| repo.metadata.branch.as_str())
            .filter(|branch| !branch.is_empty());
        let name = root.as_ref().map(|root| {
            root.file_name()
                .unwrap_or(root.as_os_str())
                .to_string_lossy()
                .into_owned()
        });
        let label = match (name, branch) {
            (Some(name), Some("(detached)")) => format!("{name} · {}", tr("Detached HEAD")),
            (Some(name), Some(branch)) => format!("{name} · {branch}"),
            (Some(name), None) => name,
            _ => tr("No Git repository"),
        };
        let roots = self.roots.clone();
        let catalog = self.catalog.clone();
        let owner = cx.entity();
        h_flex()
            .w_full()
            .min_w_0()
            .px_2()
            .gap_1()
            .child(
                Button::new("git-repository-picker")
                    .small()
                    .ghost()
                    .flex_1()
                    .min_w_0()
                    .justify_start()
                    .accessibility_label(label.clone())
                    .child(div().min_w_0().truncate().child(label))
                    .disabled(root.is_none())
                    .dropdown_menu(move |mut menu, _, _| {
                        for path in &roots {
                            let path = path.clone();
                            let owner = owner.clone();
                            menu = menu.item(
                                PopupMenuItem::new(path.display().to_string())
                                    .checked(root.as_ref() == Some(&path))
                                    .on_click(move |_, _, cx| {
                                        owner.update(cx, |view, cx| {
                                            view.activate_root(path.clone(), cx)
                                        })
                                    }),
                            );
                        }
                        for tree in &catalog.worktrees {
                            if tree.prunable || roots.contains(&tree.path) {
                                continue;
                            }
                            let path = tree.path.clone();
                            let owner = owner.clone();
                            menu = menu.item(
                                PopupMenuItem::new(if tree.branch.is_empty() {
                                    path.display().to_string()
                                } else {
                                    format!("{} · {}", tree.branch, path.display())
                                })
                                .checked(root.as_ref() == Some(&path))
                                .on_click(move |_, _, cx| {
                                    owner
                                        .update(cx, |view, cx| view.activate_root(path.clone(), cx))
                                }),
                            );
                        }
                        if let Some(path) = &root {
                            let open_path = path.clone();
                            let copy_path = path.display().to_string();
                            let owner = owner.clone();
                            menu = menu
                                .separator()
                                .item(PopupMenuItem::new(tr("Open worktree")).on_click(
                                    move |_, _, cx| {
                                        owner.update(cx, |_, cx| {
                                            cx.emit(OpenWorktree(open_path.clone()))
                                        });
                                    },
                                ))
                                .item(PopupMenuItem::new(tr("Copy Absolute Path")).on_click(
                                    move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copy_path.clone(),
                                        ));
                                    },
                                ));
                        }
                        menu.scrollable(true).max_h(px(320.))
                    }),
            )
            .child(
                icon_action(Button::new("git-refresh"), IconName::RefreshCw, "Refresh")
                    .disabled(self.loading || self.active_root.is_none())
                    .on_click(cx.listener(|view, _, _, cx| view.refresh(cx))),
            )
            .child(
                icon_action(
                    Button::new("git-fetch"),
                    IconName::Download,
                    "Fetch updates from all remotes",
                )
                .disabled(
                    self.loading || self.fetch_task.is_some() || self.catalog.remotes.is_empty(),
                )
                .on_click(cx.listener(|view, _, _, cx| view.fetch(cx))),
            )
    }

    fn fetch(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.active_root.clone() else {
            return;
        };
        if self.fetch_task.is_some() {
            return;
        }
        self.fetch_error = None;
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
                        Err(error) => view.fetch_error = Some(format!("{error:#}").into()),
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn revision_menu(&self, cx: &Context<Self>) -> impl IntoElement {
        let catalog = self.catalog.clone();
        let owner = cx.entity();
        let root = self.active_root.clone();
        Button::new("git-compare")
            .small()
            .ghost()
            .label(tr("Compare revisions"))
            .disabled(root.is_none() || self.loading)
            .dropdown_menu(move |mut menu, _, _| {
                let comparison_owner = owner.clone();
                let comparison_root = root.clone();
                menu = menu.item(PopupMenuItem::new(tr("Choose two revisions…")).on_click(
                    move |_, window, cx| {
                        comparison_owner.update(cx, |view, cx| {
                            if view.active_root == comparison_root {
                                view.compare_dialog(window, cx);
                            }
                        });
                    },
                ));
                if !catalog.references.is_empty() {
                    menu = menu.separator();
                }
                for reference in &catalog.references {
                    let label = if reference.tag.is_some() {
                        reference.name.clone()
                    } else {
                        format!("HEAD → {}", reference.name)
                    };
                    let comparison = Comparison::for_reference(reference);
                    let owner = owner.clone();
                    let root = root.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        owner.update(cx, |view, cx| {
                            if view.active_root == root {
                                view.open_comparison(comparison.clone(), cx);
                            }
                        });
                    }));
                }
                if !catalog.stashes.is_empty() {
                    menu = menu.separator();
                    for stash in &catalog.stashes {
                        let id = stash.id.clone();
                        let owner = owner.clone();
                        let root = root.clone();
                        menu = menu.item(
                            PopupMenuItem::new(format!("{} · {}", stash.name, stash.subject))
                                .on_click(move |_, _, cx| {
                                    owner.update(cx, |view, cx| {
                                        if view.active_root == root {
                                            view.open_comparison(Comparison::Stash(id.clone()), cx);
                                        }
                                    })
                                }),
                        );
                    }
                }
                menu.scrollable(true).max_h(px(320.))
            })
    }
    fn compare_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.active_root.clone();
        let owner = cx.entity().downgrade();
        let base = cx.new(|cx| {
            let mut input = InputState::new(window, cx);
            let initial = self.catalog.default_branch.as_deref().unwrap_or("HEAD");
            input.set_value(initial, window, cx);
            input
        });
        let target = cx.new(|cx| {
            let mut input = InputState::new(window, cx);
            input.set_value("HEAD", window, cx);
            input
        });
        window.open_dialog(cx, move |dialog, window, cx| {
            let owner = owner.clone();
            let root = root.clone();
            let base_input = base.clone();
            let target_input = target.clone();
            dialog
                .title(tr("Compare revisions"))
                .width(px(380.))
                .child(
                    v_flex()
                        .gap_2()
                        .child(div().text_sm().child(tr("Base revision")))
                        .child(
                            crate::views::form_input(&base, window, cx)
                                .aria_label(tr("Base revision")),
                        )
                        .child(div().text_sm().child(tr("Target revision")))
                        .child(
                            crate::views::form_input(&target, window, cx)
                                .aria_label(tr("Target revision")),
                        ),
                )
                .button_props(
                    gpui_kit::component::dialog::DialogButtonProps::default()
                        .ok_text(tr("Compare revisions"))
                        .cancel_text(tr("Cancel")),
                )
                .on_ok(move |_, _, cx| {
                    let left = base_input.read(cx).value().trim().to_owned();
                    let right = target_input.read(cx).value().trim().to_owned();
                    if left.is_empty() || right.is_empty() {
                        return false;
                    }
                    let _ = owner.update(cx, |view, cx| {
                        if view.active_root == root {
                            view.open_comparison(Comparison::Between(left, right), cx);
                        }
                    });
                    true
                })
        });
    }

    pub(super) fn diff_options(&self, parsed: bool, cx: &Context<Self>) -> impl IntoElement {
        let owner = cx.entity();
        let diff = self.diff.clone();
        let wrap = self.soft_wrap;
        icon_action(Button::new("git-diff-options"), IconName::Ellipsis, "More").dropdown_menu(
            move |mut menu, _, _| {
                let refresh = owner.clone();
                menu = menu.item(PopupMenuItem::new(tr("Refresh")).on_click(move |_, _, cx| {
                    refresh.update(cx, |view, cx| view.refresh(cx));
                }));
                let toggle_wrap = owner.clone();
                menu = menu.item(
                    PopupMenuItem::new(tr("Wrap"))
                        .checked(wrap)
                        .disabled(!parsed)
                        .on_click(move |_, _, cx| {
                            toggle_wrap.update(cx, |view, cx| {
                                view.soft_wrap = !view.soft_wrap;
                                cx.notify();
                            });
                        }),
                );
                for (label, collapsed) in [("Collapse all", true), ("Expand all", false)] {
                    let diff = diff.clone();
                    menu = menu.item(PopupMenuItem::new(tr(label)).disabled(!parsed).on_click(
                        move |_, _, cx| {
                            diff.update(cx, |state, cx| {
                                let paths: Vec<_> = state
                                    .files()
                                    .iter()
                                    .filter(|file| {
                                        state.is_file_collapsed(file.path()) != collapsed
                                    })
                                    .map(|file| file.path().clone())
                                    .collect();
                                for path in paths {
                                    state.set_file_collapsed(&path, collapsed, cx);
                                }
                            });
                        },
                    ));
                }
                menu
            },
        )
    }
}
