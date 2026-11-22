use super::*;
use crate::views::settings_controls::{field_choice, field_dropdown};
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

pub(super) fn operation_icon(op: &Operation) -> IconName {
    match op {
        Operation::Merge(_) => IconName::GitMerge,
        Operation::Rebase(_) | Operation::Switch(_) => IconName::GitBranch,
        Operation::CherryPick(_) => IconName::GitCommitHorizontal,
        Operation::Revert(_)
        | Operation::Reset { .. }
        | Operation::RestoreFile { .. }
        | Operation::Discard(_) => IconName::Undo2,
        Operation::DeleteBranch(_)
        | Operation::DeleteTag(_)
        | Operation::DeleteRemoteTag { .. }
        | Operation::DeleteRemoteBranch { .. }
        | Operation::StashDrop { .. }
        | Operation::RemoveRemote(_) => IconName::Trash,
        Operation::Push(_) | Operation::PushTag { .. } | Operation::ForcePush { .. } => {
            IconName::ArrowUp
        }
        Operation::Fetch(_) => IconName::Download,
        Operation::StashApply(_) => IconName::Copy,
        Operation::StashPop { .. } => IconName::ArchiveRestore,
        Operation::Stage(_) => IconName::Plus,
        Operation::Unstage(_) => IconName::Minus,
        Operation::Ignore(_) => IconName::EyeOff,
        _ => IconName::Check,
    }
}

#[derive(Clone)]
pub(super) enum Form {
    Branch,
    Rename(String),
    Tag,
    Remote,
    Worktree,
    Stash,
    Compare,
}
impl Form {
    fn title(&self) -> &'static str {
        match self {
            Self::Branch => "Create branch",
            Self::Rename(_) => "Rename branch",
            Self::Tag => "Create tag",
            Self::Remote => "Add remote",
            Self::Worktree => "Add worktree",
            Self::Stash => "Save stash",
            Self::Compare => "Compare revisions",
        }
    }
    fn revision_field(&self, index: usize) -> bool {
        matches!(self, Self::Compare)
            || (index == 1 && matches!(self, Self::Branch | Self::Tag | Self::Worktree))
    }
    fn fields(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::Branch => vec![
                ("Branch name", String::new()),
                ("From branch", String::new()),
            ],
            Self::Rename(old) => vec![("Branch name", old.clone())],
            Self::Tag => vec![("Tag name", String::new()), ("Revision", String::new())],
            Self::Remote => vec![
                ("Remote name", String::new()),
                ("Remote URL", String::new()),
            ],
            Self::Worktree => vec![
                ("Absolute worktree path", String::new()),
                ("Revision", String::new()),
            ],
            Self::Stash => vec![("Stash message", String::new())],
            Self::Compare => vec![
                ("Base revision", String::new()),
                ("Target revision", String::new()),
            ],
        }
    }
}

impl Changes {
    pub(super) fn busy(&self, cx: &App) -> bool {
        !git::enabled()
            || !self.available
            || self.operation_task.is_some()
            || crate::app::updates::busy(cx)
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
    pub(super) fn operation_blocked(&self, operation: &Operation, cx: &App) -> bool {
        self.busy(cx)
            || self.active_root.as_ref().is_none_or(|root| {
                crate::app::git_operation::blocked(root, self.remote.as_ref(), operation, cx)
            })
    }

    fn begin(
        &mut self,
        root: PathBuf,
        source: ElementId,
        operation: Operation,
        cx: &mut Context<Self>,
    ) {
        self.operation_error = None;
        let key = (
            root.clone(),
            self.remote.as_ref().map(|s| s.connection.id.clone()),
            source.clone(),
        );
        let owner = cx.entity().downgrade();
        if let Err(error) = crate::app::git_operation::start(
            root,
            self.remote.clone(),
            operation.clone(),
            source,
            owner,
            cx,
        ) {
            self.operation_error = Some(format!("{}\n{error:#}", tr(operation.label())).into());
        } else {
            self.feedback.insert(key, None);
        }
        cx.notify();
    }

    pub(super) fn request(
        &mut self,
        source: ElementId,
        operation: Operation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.operation_blocked(&operation, cx) {
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
                        view.begin(root, source, operation, cx);
                    }
                    cx.notify();
                });
            }));
        } else {
            self.begin(root, source, operation, cx);
        }
    }

    pub(crate) fn operation_finished(
        &mut self,
        root: PathBuf,
        connection: Option<String>,
        source: ElementId,
        operation: &Operation,
        result: anyhow::Result<()>,
        cx: &mut Context<Self>,
    ) {
        let active = self.active_root.as_ref() == Some(&root)
            && connection.as_deref() == self.remote.as_ref().map(|s| s.connection.id.as_str());
        let key = (root.clone(), connection, source);
        match result {
            Ok(()) => {
                if let Operation::Init(path) | Operation::Clone { path, .. } = operation {
                    cx.emit(OpenFile(path.clone()));
                }
                if let Operation::Commit { message, .. } = operation {
                    if active && self.message_root.as_ref() == Some(&root) {
                        self.clear_message = Some(message.clone());
                        self.amend = false;
                    } else if let Some((draft, amend)) = self.drafts.get_mut(&root)
                        && draft == message
                    {
                        draft.clear();
                        *amend = false;
                    }
                }
                self.show_completed(key, cx);
            }
            Err(error) => {
                self.feedback.remove(&key);
                self.operation_error = Some(format!("{}\n{error:#}", tr(operation.label())).into());
            }
        }
        if active && self.monitoring {
            self.refresh(cx);
        }
        cx.notify();
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

    pub(super) fn operation_button(
        &self,
        id: impl Into<ElementId>,
        operation: Operation,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let id = id.into();
        let button = Button::new(id.clone()).small();
        let button = match &operation {
            Operation::Commit { .. } => icon_action(button, IconName::Send, "Commit"),
            Operation::Stage(Some(change)) if change.status == 'U' => {
                button.label(tr("Mark resolved"))
            }
            Operation::Stage(Some(_)) => icon_action(button, IconName::Plus, "Stage file"),
            Operation::Unstage(Some(_)) => icon_action(button, IconName::Minus, "Unstage"),
            Operation::Fetch(_) => icon_action(button, IconName::Download, operation.label()),
            Operation::Pull(_) => icon_action(button, IconName::ArrowDown, operation.label()),
            Operation::Push(_) => icon_action(button, IconName::ArrowUp, operation.label()),
            Operation::Stage(None) => icon_action(button, IconName::ListPlus, operation.label()),
            Operation::Unstage(None) => icon_action(button, IconName::ListMinus, operation.label()),
            Operation::Ignore(_) => icon_action(button, IconName::EyeOff, operation.label()),
            Operation::Discard(_) => icon_action(button, IconName::Undo2, operation.label()),
            Operation::Submodules => icon_action(button, IconName::Package, operation.label()),
            Operation::ConflictSide { incoming, .. } => icon_action(
                button,
                if *incoming {
                    IconName::ArrowRight
                } else {
                    IconName::ArrowLeft
                },
                operation.label(),
            ),
            Operation::Continue(_) => icon_action(button, IconName::Check, operation.label()),
            Operation::Abort(_) => icon_action(button, IconName::X, operation.label()),
            _ => button.label(tr(operation.label())),
        };
        self.feedback_button(id.clone(), button)
            .disabled(disabled || self.operation_blocked(&operation, cx))
            .on_click(cx.listener(move |view, _, window, cx| {
                view.request(id.clone(), operation.clone(), window, cx)
            }))
    }

    pub(super) fn branch_picker(
        &self,
        branch: &str,
        branches: Arc<Vec<String>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner = cx.entity().downgrade();
        let root = self.active_root.clone();
        let current = branch.to_owned();
        self.feedback_button(
            "git-branch-picker".into(),
            Button::new("git-branch-picker")
                .small()
                .ghost()
                .icon(IconName::GitBranch)
                .label(current.clone()),
        )
        .tooltip(tr("Switch branch"))
        .accessibility_label(format!("{} · {current}", tr("Switch branch")))
        .disabled(self.busy(cx) || branches.is_empty())
        .dropdown_menu(move |mut menu, _, _| {
            for name in branches.iter() {
                let owner = owner.clone();
                let root = root.clone();
                let op = Operation::Switch(format!("refs/heads/{name}"));
                menu = menu.item(
                    PopupMenuItem::new(name.clone())
                        .checked(*name == current)
                        .disabled(*name == current)
                        .on_click(move |_, window, cx| {
                            let _ = owner.update(cx, |view, cx| {
                                if view.active_root == root {
                                    view.request(
                                        "git-branch-picker".into(),
                                        op.clone(),
                                        window,
                                        cx,
                                    );
                                }
                            });
                        }),
                );
            }
            menu.scrollable(true).max_h(px(320.))
        })
        .into_any_element()
    }

    pub(super) fn load_identity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.identity_task = None;
        self.identity_ready = false;
        self.identity_root = self.active_root.clone();
        let Some(root) = self.active_root.clone() else {
            return;
        };
        let remote = self.remote.clone();
        let (cancel, guard) = Cancellation::task();
        let job = cx
            .background_executor()
            .spawn(async move { git::commit_identity(&root, remote.as_ref(), &cancel) });
        self.identity_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let result = job.await;
            let _ = view.update_in(cx, |view, window, cx| {
                if view.identity_root != view.active_root {
                    return;
                }
                match result {
                    Ok(values) => {
                        for (input, value) in view.identity.iter().zip(values) {
                            input.update(cx, |input, cx| input.set_value(value, window, cx));
                        }
                        view.identity_ready = true;
                    }
                    Err(error) => view.operation_error = Some(error.to_string().into()),
                }
                cx.notify();
            });
        }));
    }

    pub(super) fn settings_view(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        use gpui_kit::component::switch::Switch;
        let prefs = Self::preferences(cx);
        let mut panel = v_flex()
            .id("git-settings-panel")
            .min_h_0()
            .p_2()
            .gap_2()
            .overflow_y_scroll();
        for (id, label, checked) in [
            (
                "git-auto-refresh",
                "Refresh status every minute",
                prefs.auto_refresh,
            ),
            ("git-worktrees", "Enable worktrees", prefs.worktrees),
        ] {
            panel = panel.child(
                h_flex()
                    .w_full()
                    .min_h(px(30.))
                    .gap_2()
                    .child(div().flex_1().text_sm().child(tr(label)))
                    .child(
                        Switch::new(id)
                            .small()
                            .checked(checked)
                            .accessibility_label(tr(label))
                            .on_click(cx.listener(move |view, enabled, _, cx| {
                                let mut prefs = Self::preferences(cx);
                                if id == "git-auto-refresh" {
                                    prefs.auto_refresh = *enabled;
                                } else {
                                    prefs.worktrees = *enabled;
                                }
                                view.save_preferences(prefs, cx);
                            })),
                    ),
            );
        }
        let strategies = [(false, "Fast-forward only"), (true, "Rebase")];
        let strategy = tr(if prefs.pull_rebase {
            "Rebase"
        } else {
            "Fast-forward only"
        });
        let owner = cx.entity().downgrade();
        let picker = field_dropdown(
            Button::new("git-pull-strategy")
                .small()
                .outline()
                .w_full()
                .label(strategy.clone())
                .dropdown_caret(true)
                .accessibility_label(format!("{} · {strategy}", tr("Pull strategy"))),
            window,
            cx,
            move |mut menu| {
                for (rebase, label) in strategies {
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(tr(label))
                            .checked(prefs.pull_rebase == rebase)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |view, cx| {
                                    let mut prefs = Self::preferences(cx);
                                    prefs.pull_rebase = rebase;
                                    view.save_preferences(prefs, cx);
                                });
                            }),
                    );
                }
                menu
            },
        )
        .w(rems(7.5))
        .max_w(relative(0.65));
        panel = panel.child(
            h_flex()
                .w_full()
                .min_h(px(30.))
                .gap_2()
                .child(div().flex_1().text_sm().child(tr("Pull strategy")))
                .child(picker),
        );
        let ready = self.identity_ready && self.identity_root == self.active_root && !self.busy(cx);
        let save = self
            .feedback_button(
                "git-save-identity".into(),
                icon_action(Button::new("git-save-identity"), IconName::Check, "Save"),
            )
            .disabled(!ready)
            .on_click(cx.listener(|view, _, window, cx| {
                let [name, email] = view
                    .identity
                    .each_ref()
                    .map(|input| input.read(cx).value().trim().to_owned());
                if name.is_empty() || email.is_empty() {
                    view.operation_error = Some(tr("Complete the required fields").into());
                    cx.notify();
                    return;
                }
                view.request(
                    "git-save-identity".into(),
                    Operation::Identity { name, email },
                    window,
                    cx,
                );
            }));
        panel = panel.child(
            h_flex()
                .w_full()
                .border_t_1()
                .border_color(cx.theme().border)
                .pt_2()
                .child(div().flex_1().text_sm().child(tr("Commit identity")))
                .child(save),
        );
        for (field, label) in self.identity.iter().zip(["Author name", "Author email"]) {
            panel = panel.child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .child(div().w(rems(5.)).flex_shrink_0().text_xs().child(tr(label)))
                    .child(
                        div().flex_1().min_w_0().child(
                            crate::views::form_input(field, window, cx)
                                .aria_label(tr(label))
                                .disabled(!ready),
                        ),
                    ),
            );
        }
        panel.into_any_element()
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
            Form::Tag => IconName::Tag,
            Form::Branch => IconName::GitBranchPlus,
            Form::Rename(_) => IconName::GitBranch,
            Form::Remote => IconName::Server,
            Form::Worktree => IconName::FolderPlus,
            Form::Stash => IconName::Archive,
        };
        self.feedback_button(id.into(), icon_action(Button::new(id), icon, form.title()))
            .disabled(
                self.busy(cx)
                    || self.active_root.is_none()
                    || (self.browsing && form.revision_field(1)),
            )
            .on_click(cx.listener(move |view, _, window, cx| {
                view.form(id.into(), form.clone(), window, cx)
            }))
    }

    pub(super) fn form(
        &mut self,
        source: ElementId,
        form: Form,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                let value = if form.revision_field(index) {
                    self.catalog
                        .default_branch
                        .clone()
                        .unwrap_or_else(|| "HEAD".into())
                } else {
                    saved.get(index).cloned().unwrap_or(value)
                };
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
        let branch_only = matches!(form, Form::Branch);
        let mut revisions = Vec::new();
        if !branch_only {
            revisions.push(("HEAD".to_string(), "HEAD".to_string()));
        }
        revisions.extend(
            self.catalog
                .references
                .iter()
                .filter(|r| !branch_only || !r.full.starts_with("refs/tags/"))
                .map(|r| (r.name.clone(), r.full.clone())),
        );
        if !branch_only {
            revisions.extend(self.commits.iter().map(|c| {
                (
                    format!("{}  {}", &c.id[..8.min(c.id.len())], c.subject),
                    c.id.clone(),
                )
            }));
        }
        let revisions = Arc::new(revisions);
        let focus = fields
            .first()
            .filter(|_| !form.revision_field(0))
            .map(|(_, input)| input.read(cx).focus_handle(cx));
        window.open_dialog(cx, move |dialog, window, cx| {
            let source = source.clone();
            let fields = fields.clone();
            let owner = owner.clone();
            let form = form.clone();
            let root = root.clone();
            let key = key.clone();
            let validation = validation.clone();
            let mut dialog = dialog.title(tr(form.title())).width(px(480.));
            for (index, (label, input)) in fields.iter().enumerate() {
                if form.revision_field(index) {
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
                                            .child(
                                                div().min_w_0().flex_1().truncate().child(display),
                                            )
                                            .child(
                                                gpui_kit::component::Icon::new(
                                                    IconName::ChevronDown,
                                                )
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
                    continue;
                }
                dialog = dialog.child(
                    v_flex()
                        .gap_1()
                        .child(div().text_sm().child(tr(label)))
                        .child(crate::views::form_input(input, window, cx).aria_label(tr(label))),
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
                        Form::Worktree => Some(0),
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
                            Form::Branch => Operation::CreateBranch {
                                name: values[0].clone(),
                                from: values[1].clone(),
                            },
                            Form::Rename(old) => {
                                Operation::RenameBranch(old.clone(), values[0].clone())
                            }
                            Form::Tag => Operation::Tag(values[0].clone(), values[1].clone()),
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
                        view.request(source.clone(), operation, window, cx);
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
                "git-save-resolution".into(),
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
            "git-stage-blocks".into(),
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
