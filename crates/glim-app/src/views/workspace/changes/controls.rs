use super::*;
use gpui_kit::component::{
    WindowExt,
    menu::{DropdownMenu, PopupMenuItem},
};

use gpui_kit::assets::IconName;

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
pub(super) fn icon_action(button: Button, icon: IconName, label: &str) -> Button {
    let label = tr(label);
    button
        .ghost()
        .small()
        .icon(icon)
        .tooltip(label.clone())
        .accessibility_label(label)
}

#[derive(Clone)]
pub(super) enum Form {
    Init,
    Identity,
    Clone,
    Branch,
    Rename(String),
    Tag(String),
    Remote,
    Worktree,
    Stash,
    Compare,
}
impl Form {
    fn title(&self) -> &'static str {
        match self {
            Self::Init => "Initialize repository",
            Self::Identity => "Commit identity",
            Self::Clone => "Clone repository",
            Self::Branch => "Create branch",
            Self::Rename(_) => "Rename branch",
            Self::Tag(_) => "Create tag",
            Self::Remote => "Add remote",
            Self::Worktree => "Add worktree",
            Self::Stash => "Save stash",
            Self::Compare => "Compare revisions",
        }
    }
    fn fields(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::Identity => vec![
                ("Author name", String::new()),
                ("Author email", String::new()),
            ],
            Self::Init => vec![("Absolute repository path", String::new())],
            Self::Clone => vec![
                ("Remote URL", String::new()),
                ("Absolute repository path", String::new()),
            ],
            Self::Branch => vec![("Branch name", String::new())],
            Self::Rename(old) => vec![("Branch name", old.clone())],
            Self::Tag(target) => vec![("Tag name", String::new()), ("Revision", target.clone())],
            Self::Remote => vec![
                ("Remote name", String::new()),
                ("Remote URL", String::new()),
            ],
            Self::Worktree => vec![
                ("Absolute worktree path", String::new()),
                ("Revision", "HEAD".into()),
            ],
            Self::Stash => vec![("Stash message", String::new())],
            Self::Compare => vec![
                ("Base revision", "HEAD~1".into()),
                ("Target revision", "HEAD".into()),
            ],
        }
    }
}

impl Changes {
    pub(super) fn busy(&self, cx: &App) -> bool {
        !git::enabled()
            || !self.available
            || self.operation_busy
            || self.operation_task.is_some()
            || crate::app::workspace_editors_locked(cx)
            || crate::app::git_operation::busy(cx)
            || crate::app::updates::busy(cx)
    }

    fn begin(&mut self, root: PathBuf, operation: Operation, cx: &mut Context<Self>) {
        self.operation_busy = true;
        self.operation_error = None;
        self.operation_label = Some(tr(operation.label()).into());
        self.notice_task = None;
        let owner = cx.entity().downgrade();
        if let Err(error) = crate::app::git_operation::start(
            root,
            self.remote.clone(),
            operation.clone(),
            owner,
            cx,
        ) {
            self.operation_finished(&operation, Err(error), cx);
        }
        cx.notify();
    }

    pub(super) fn request(
        &mut self,
        operation: Operation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy(cx) {
            return;
        }
        let root = match &operation {
            Operation::Init(path) | Operation::Clone { path, .. } => path.clone(),
            _ => match self.active_root.clone() {
                Some(root) => root,
                None => return,
            },
        };
        if operation.confirmation() {
            let title = tr(operation.label());
            let detail = format!("{}\n\n{}", root.display(), operation.details());
            let answer = window.prompt(
                PromptLevel::Warning,
                &title,
                Some(&detail),
                &[tr("Cancel").as_str(), title.as_str()],
                cx,
            );
            self.operation_task = Some(cx.spawn_in(window, async move |view, cx| {
                let accepted = answer.await == Ok(1);
                let _ = view.update_in(cx, |view, _, cx| {
                    view.operation_task = None;
                    if accepted && !view.busy(cx) && view.active_root.as_ref() == Some(&root) {
                        view.begin(root, operation, cx);
                    }
                    cx.notify();
                });
            }));
        } else {
            self.begin(root, operation, cx);
        }
    }

    pub(crate) fn operation_finished(
        &mut self,
        operation: &Operation,
        result: anyhow::Result<()>,
        cx: &mut Context<Self>,
    ) {
        self.operation_busy = false;
        match result {
            Ok(()) => {
                if let Operation::Init(path) | Operation::Clone { path, .. } = operation {
                    cx.emit(OpenFile(path.clone()));
                }
                if let Operation::Commit { message, .. } = operation {
                    self.clear_message = Some(message.clone());
                    self.amend = false;
                }
                self.operation_label =
                    Some(format!("{} · {}", tr(operation.label()), tr("Done")).into());
                self.notice_task = Some(cx.spawn(async move |view, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(3))
                        .await;
                    let _ = view.update(cx, |view, cx| {
                        view.operation_label = None;
                        cx.notify();
                    });
                }));
            }
            Err(error) => {
                self.operation_label = None;
                self.operation_error = Some(format!("{}\n{error:#}", tr(operation.label())).into());
            }
        }
        self.refresh(cx);
    }

    pub(super) fn operation_button(
        &self,
        id: impl Into<ElementId>,
        operation: Operation,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let button = Button::new(id).small();
        let button = match &operation {
            Operation::Commit { .. } => icon_action(button, IconName::Send, "Commit"),
            Operation::Stage(Some(change)) if change.status == 'U' => {
                button.label(tr("Mark resolved"))
            }
            Operation::Stage(Some(_)) => icon_action(button, IconName::Plus, "Stage file"),
            Operation::Unstage(Some(_)) => icon_action(button, IconName::Minus, "Unstage"),
            Operation::Fetch(_) => icon_action(button, IconName::Download, operation.label()),
            _ => button.label(tr(operation.label())),
        };
        button
            .disabled(disabled || self.busy(cx) || self.active_root.is_none())
            .on_click(
                cx.listener(move |view, _, window, cx| view.request(operation.clone(), window, cx)),
            )
    }

    pub(super) fn operation_menu(
        &self,
        id: &'static str,
        label: &'static str,
        operations: Vec<Operation>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner = cx.entity().downgrade();
        let root = self.active_root.clone();
        let button = Button::new(id).small();
        let button = match label {
            "Sync" => icon_action(button, IconName::ArrowUpDown, label),
            "Branch actions" | "Stash actions" | "File actions" | "Commit actions" => {
                icon_action(button, IconName::Ellipsis, label)
            }
            "Reset to commit" => icon_action(button, IconName::Undo2, label),
            "All files" => icon_action(button, IconName::Files, label),
            _ => button.label(tr(label)).icon(IconName::ChevronDown),
        };
        button
            .disabled(self.busy(cx) || operations.is_empty())
            .dropdown_menu(move |mut menu, _, _| {
                for operation in &operations {
                    let operation = operation.clone();
                    let owner = owner.clone();
                    let root = root.clone();
                    menu = menu.item(PopupMenuItem::new(operation.menu_label()).on_click(
                        move |_, window, cx| {
                            let _ = owner.update(cx, |view, cx| {
                                if view.active_root == root {
                                    view.request(operation.clone(), window, cx);
                                }
                            });
                        },
                    ));
                }
                menu
            })
            .into_any_element()
    }

    pub(super) fn repository_menu(
        &self,
        branch: &str,
        upstream: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner = cx.entity().downgrade();
        let has_repository = self.active_root.is_some();
        let hint = if upstream.is_empty() {
            branch.to_owned()
        } else {
            format!("{branch} → {upstream}")
        };
        Button::new("git-repository-menu")
            .small()
            .ghost()
            .min_w_0()
            .justify_start()
            .icon(IconName::GitBranch)
            .child(div().truncate().text_sm().child(branch.to_owned()))
            .tooltip(hint.clone())
            .accessibility_label(format!("{} · {hint}", tr("Repository")))
            .disabled(self.busy(cx))
            .dropdown_menu(move |mut menu, _, _| {
                let refresh = owner.clone();
                menu = menu.item(PopupMenuItem::new(tr("Refresh")).on_click(move |_, _, cx| {
                    let _ = refresh.update(cx, |view, cx| view.refresh(cx));
                }));
                for form in [Form::Clone, Form::Init] {
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(tr(form.title())).on_click(
                        move |_, window, cx| {
                            let _ =
                                owner.update(cx, |view, cx| view.form(form.clone(), window, cx));
                        },
                    ));
                }
                if has_repository {
                    for operation in [Operation::Submodules] {
                        let owner = owner.clone();
                        menu = menu.item(PopupMenuItem::new(operation.menu_label()).on_click(
                            move |_, window, cx| {
                                let _ = owner.update(cx, |view, cx| {
                                    view.request(operation.clone(), window, cx)
                                });
                            },
                        ));
                    }
                }
                menu
            })
            .into_any_element()
    }

    pub(super) fn settings_menu(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let owner = cx.entity().downgrade();
        let prefs = Self::preferences(cx);
        let identity_disabled = self.busy(cx) || self.active_root.is_none();
        navigation_button("git-settings", IconName::Settings, "Settings", "Settings").dropdown_menu(
            move |menu, _, _| {
                let identity_owner = owner.clone();
                let mut menu = menu
                    .item(
                        PopupMenuItem::new(tr(Form::Identity.title()))
                            .disabled(identity_disabled)
                            .on_click(move |_, window, cx| {
                                let _ = identity_owner
                                    .update(cx, |view, cx| view.form(Form::Identity, window, cx));
                            }),
                    )
                    .separator();
                for (label, checked) in [
                    ("Refresh status every minute", prefs.auto_refresh),
                    ("Enable worktrees", prefs.worktrees),
                ] {
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(tr(label)).checked(checked).on_click(
                        move |_, _, cx| {
                            let _ = owner.update(cx, |view, cx| {
                                let mut prefs = Self::preferences(cx);
                                if label == "Enable worktrees" {
                                    prefs.worktrees = !prefs.worktrees;
                                } else {
                                    prefs.auto_refresh = !prefs.auto_refresh;
                                }
                                view.save_preferences(prefs, cx);
                            });
                        },
                    ));
                }
                menu
            },
        )
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

    pub(super) fn form_button(
        &self,
        id: &'static str,
        form: Form,
        cx: &mut Context<Self>,
    ) -> Button {
        let icon = match &form {
            Form::Compare => IconName::GitCompareArrows,
            Form::Tag(_) => IconName::Tag,
            Form::Branch | Form::Rename(_) => IconName::GitBranch,
            Form::Remote => IconName::Globe,
            Form::Worktree | Form::Init | Form::Clone => IconName::FolderPlus,
            Form::Stash => IconName::Archive,
            Form::Identity => IconName::Settings2,
        };
        icon_action(Button::new(id), icon, form.title())
            .disabled(
                self.busy(cx)
                    || (self.active_root.is_none() && !matches!(form, Form::Init | Form::Clone)),
            )
            .on_click(cx.listener(move |view, _, window, cx| view.form(form.clone(), window, cx)))
    }

    fn form(&mut self, form: Form, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy(cx) {
            return;
        }
        let key = format!("{}:{:?}", form.title(), self.active_root);
        let saved = self.form_values.get(&key).cloned().unwrap_or_default();
        let fields: Vec<_> = form
            .fields()
            .into_iter()
            .enumerate()
            .map(|(index, (label, value))| {
                let value = saved.get(index).cloned().unwrap_or(value);
                let input = cx.new(|cx| {
                    let mut state = InputState::new(window, cx);
                    state.set_value(value, window, cx);
                    state
                });
                (label, input)
            })
            .collect();
        let owner = cx.entity().downgrade();
        let root = self.active_root.clone();
        let validation = std::rc::Rc::new(std::cell::RefCell::new(None::<String>));
        let mut revisions = vec![("HEAD".to_string(), "HEAD".to_string())];
        revisions.extend(
            self.catalog
                .references
                .iter()
                .map(|r| (r.name.clone(), r.full.clone())),
        );
        revisions.extend(self.commits.iter().map(|c| {
            (
                format!("{}  {}", &c.id[..8.min(c.id.len())], c.subject),
                c.id.clone(),
            )
        }));
        let revisions = Arc::new(revisions);
        let focus = fields
            .first()
            .map(|(_, input)| input.read(cx).focus_handle(cx));
        window.open_dialog(cx, move |dialog, window, cx| {
            let fields = fields.clone();
            let owner = owner.clone();
            let form = form.clone();
            let root = root.clone();
            let key = key.clone();
            let validation = validation.clone();
            let mut dialog = dialog.title(tr(form.title())).width(px(480.));
            for (index, (label, input)) in fields.iter().enumerate() {
                dialog = dialog.child(
                    v_flex()
                        .gap_1()
                        .child(div().text_sm().child(tr(label)))
                        .child(
                            h_flex()
                                .gap_1()
                                .child(
                                    div().flex_1().min_w_0().child(
                                        crate::views::form_input(input, window, cx)
                                            .aria_label(tr(label)),
                                    ),
                                )
                                .when(
                                    matches!(
                                        *label,
                                        "Revision" | "Base revision" | "Target revision"
                                    ),
                                    |row| {
                                        let revisions = revisions.clone();
                                        let input = input.clone();
                                        row.child(
                                            icon_action(
                                                Button::new(("revision-picker", index)),
                                                IconName::ChevronDown,
                                                label,
                                            )
                                            .dropdown_menu(move |mut menu, _, _| {
                                                for (label, revision) in revisions.iter() {
                                                    let revision = revision.clone();
                                                    let input = input.clone();
                                                    menu = menu.item(
                                                        PopupMenuItem::new(label.clone()).on_click(
                                                            move |_, window, cx| {
                                                                input.update(cx, |field, cx| {
                                                                    field.set_value(
                                                                        revision.clone(),
                                                                        window,
                                                                        cx,
                                                                    )
                                                                });
                                                            },
                                                        ),
                                                    );
                                                }
                                                menu.scrollable(true).max_h(px(320.))
                                            }),
                                        )
                                    },
                                ),
                        ),
                );
            }
            if matches!(form, Form::Worktree) {
                dialog = dialog.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(
                            "Creates a detached worktree. Create a branch there before committing.",
                        )),
                );
            }
            if let Some(error) = validation.borrow().clone() {
                dialog = dialog.child(div().text_sm().text_color(cx.theme().danger).child(error));
            }
            let title = tr(form.title());
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
                        .ok_text(tr(form.title()))
                        .cancel_text(tr("Cancel")),
                )
                .on_ok(move |_, window, cx| {
                    let values: Vec<_> = fields
                        .iter()
                        .map(|(_, input)| input.read(cx).value().to_string().trim().to_owned())
                        .collect();
                    if values.iter().any(String::is_empty) {
                        *validation.borrow_mut() = Some(tr("Complete the required fields"));
                        window.refresh();
                        return false;
                    }
                    let path_index = match form {
                        Form::Init | Form::Worktree => Some(0),
                        Form::Clone => Some(1),
                        _ => None,
                    };
                    if path_index.is_some_and(|index| !PathBuf::from(&values[index]).is_absolute())
                    {
                        *validation.borrow_mut() = Some(tr("Enter an absolute folder path"));
                        window.refresh();
                        return false;
                    }
                    let _ = owner.update(cx, |view, cx| {
                        if view.active_root != root {
                            return;
                        }
                        view.form_values.insert(key.clone(), values.clone());
                        let operation = match &form {
                            Form::Identity => Operation::Identity {
                                name: values[0].clone(),
                                email: values[1].clone(),
                            },
                            Form::Init => Operation::Init(PathBuf::from(&values[0])),
                            Form::Clone => Operation::Clone {
                                url: values[0].clone(),
                                path: PathBuf::from(&values[1]),
                            },
                            Form::Branch => Operation::CreateBranch(values[0].clone()),
                            Form::Rename(old) => {
                                Operation::RenameBranch(old.clone(), values[0].clone())
                            }
                            Form::Tag(_) => Operation::Tag(values[0].clone(), values[1].clone()),
                            Form::Remote => {
                                Operation::AddRemote(values[0].clone(), values[1].clone())
                            }
                            Form::Worktree => Operation::AddWorktree {
                                path: PathBuf::from(&values[0]),
                                revision: values[1].clone(),
                            },
                            Form::Stash => Operation::StashSave(values[0].clone()),
                            Form::Compare => {
                                view.show_comparison(
                                    Comparison::Between(values[0].clone(), values[1].clone()),
                                    cx,
                                );
                                return;
                            }
                        };
                        view.request(operation, window, cx);
                    });
                    true
                })
        });
        if let Some(focus) = focus {
            window.focus(&focus, cx);
        }
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

    pub(super) fn save_resolution(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, change)) = self.selection.clone() else {
            return;
        };
        let Some(original) = self.patch_source.clone() else {
            return;
        };
        let text = self
            .diff
            .read(cx)
            .files()
            .first()
            .and_then(|file| self.diff.read(cx).resolved_text(file.path()));
        if let Some(text) = text {
            self.request(
                Operation::Resolve {
                    path: change.path,
                    original,
                    text: Arc::new(text),
                },
                window,
                cx,
            );
        }
    }

    pub(super) fn selected_blocks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, change)) = self.selection.clone() else {
            return;
        };
        let Some(patch) = self.patch_source.clone() else {
            return;
        };
        let Some(range) = self.diff.read(cx).selected_lines() else {
            return;
        };
        if range.side() != range.end_side() {
            return;
        }
        self.request(
            Operation::Hunks {
                change,
                patch,
                start: range.start().min(range.end()),
                end: range.start().max(range.end()),
                original: range.side() == gpui_kit::component::diff::DiffSide::Original,
            },
            window,
            cx,
        );
    }
}
