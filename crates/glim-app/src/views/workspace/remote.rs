use super::Workspace;
use crate::app::remote::{Connections, persist};
use glim_services::{
    locale::tr,
    remote::{Connection, RemoteWorkspace, Session},
    work::Cancellation,
    workspace::FolderSnapshot,
};
use gpui_kit::{
    component::{
        ActiveTheme, Disableable, Icon, Sizable, WindowExt,
        button::{Button, ButtonVariants},
        h_flex,
        input::{Input, InputEvent, InputState},
        menu::{DropdownMenu, PopupMenuItem},
        switch::Switch,
        v_flex,
    },
    prelude::FluentBuilder,
    *,
};
use std::{path::PathBuf, sync::atomic::Ordering};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Connections,
    Edit,
    Directories,
}

struct RemoteDialog {
    owner: WeakEntity<Workspace>,
    mode: Mode,
    connection: Connection,
    session: Option<Session>,
    fields: Vec<Entity<InputState>>,
    path: Entity<InputState>,
    entries: Vec<glim_core::DirectoryEntry>,
    selected: Option<PathBuf>,
    directory: PathBuf,
    show_hidden: bool,
    explicit_key: bool,
    busy: bool,
    message: Option<String>,
    task: Option<Task<()>>,
    _path_subscription: Subscription,
    open_after_save: bool,
    add_folder: bool,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
}

impl RemoteDialog {
    fn new(
        owner: WeakEntity<Workspace>,
        mode: Mode,
        open_after_save: bool,
        add_folder: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let path = cx.new(|cx| InputState::new(window, cx));
        let subscription = cx.subscribe_in(&path, window, |view, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                let path = PathBuf::from(view.path.read(cx).value().as_ref());
                view.browse(path, window, cx);
            }
        });
        let fields = (0..5)
            .map(|_| cx.new(|cx| InputState::new(window, cx)))
            .collect();
        Self {
            owner,
            mode,
            connection: Connection::default(),
            session: None,
            fields,
            path,
            entries: Vec::new(),
            selected: None,
            directory: PathBuf::new(),
            show_hidden: false,
            explicit_key: false,
            busy: false,
            message: None,
            task: None,
            _path_subscription: subscription,
            open_after_save,
            add_folder,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
        }
    }
    fn edit(&mut self, connection: Connection, window: &mut Window, cx: &mut Context<Self>) {
        self.task = None;
        self.busy = false;
        self.message = None;
        self.mode = Mode::Edit;
        self.explicit_key = connection.identity.is_some();
        let values = [
            connection.name.clone(),
            connection.host.clone(),
            connection.user.clone(),
            connection
                .port
                .map(|port| port.to_string())
                .unwrap_or_default(),
            connection
                .identity
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        ];
        for (field, value) in self.fields.iter().zip(values) {
            field.update(cx, |field, cx| field.set_value(value, window, cx));
        }
        self.connection = connection;
        self.fields[1].update(cx, |field, cx| field.focus(window, cx));
        cx.notify();
    }
    fn form_connection(&self, cx: &App) -> anyhow::Result<Connection> {
        let value = |index: usize| self.fields[index].read(cx).value().trim().to_owned();
        let port = value(3);
        let identity = value(4);
        let identity = if self.explicit_key {
            anyhow::ensure!(!identity.is_empty(), "Choose a private key file");
            Some(if let Some(path) = identity.strip_prefix("~/") {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(path)
            } else {
                identity.into()
            })
        } else {
            None
        };
        Ok(Connection {
            id: self.connection.id.clone(),
            name: value(0),
            host: value(1),
            user: value(2),
            port: if port.is_empty() {
                None
            } else {
                Some(
                    port.parse()
                        .map_err(|_| anyhow::anyhow!("SSH port must be between 1 and 65535"))?,
                )
            },
            identity,
            last_directory: self.connection.last_directory.clone(),
        })
    }
    fn save(&mut self, test: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut connection = match self.form_connection(cx) {
            Ok(connection) => connection,
            Err(error) => {
                self.message = Some(tr(&error.to_string()));
                cx.notify();
                return;
            }
        };
        self.task = None;
        self.busy = true;
        self.message = None;
        let (cancel, guard) = Cancellation::task();
        let work = cx.background_executor().spawn(async move {
            connection.validate()?;
            if test {
                let session = Session::connect(connection.clone(), &cancel)?;
                session.list(&session.home, &cancel)?;
            }
            anyhow::Ok(connection)
        });
        self.task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.busy = false;
                match result {
                    Err(error) => view.message = Some(tr(&error.to_string())),
                    Ok(connection) if test => {
                        view.connection.id = connection.id;
                        view.message = Some(tr("Connection successful"));
                    }
                    Ok(connection) => {
                        let catalog = &mut cx.global_mut::<Connections>().catalog;
                        if let Some(existing) = catalog
                            .connections
                            .iter_mut()
                            .find(|item| item.id == connection.id)
                        {
                            *existing = connection.clone();
                        } else {
                            catalog.connections.push(connection.clone());
                        }
                        persist(cx);
                        if view.open_after_save {
                            view.connect(connection, window, cx);
                        } else {
                            view.mode = Mode::Connections;
                            view.message = None;
                        }
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn connect(&mut self, connection: Connection, window: &mut Window, cx: &mut Context<Self>) {
        self.task = None;
        self.session = None;
        self.mode = Mode::Directories;
        self.connection = connection.clone();
        self.entries.clear();
        self.selected = None;
        self.message = None;
        self.busy = true;
        let (cancel, guard) = Cancellation::task();
        let work = cx.background_executor().spawn(async move {
            let session = Session::connect(connection.clone(), &cancel)?;
            let path = connection.last_directory.as_ref().unwrap_or(&session.home);
            let listing = session
                .list(path, &cancel)
                .or_else(|_| session.list(&session.home, &cancel))?;
            anyhow::Ok((session, listing))
        });
        self.task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.busy = false;
                match result {
                    Ok((session, (directory, entries))) => {
                        view.session = Some(session);
                        view.set_listing(directory, entries, window, cx);
                    }
                    Err(error) => view.message = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn set_listing(
        &mut self,
        directory: PathBuf,
        entries: Vec<glim_core::DirectoryEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.directory = directory;
        self.entries = entries;
        self.selected = None;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.path.update(cx, |path, cx| {
            path.set_value(self.directory.display().to_string(), window, cx)
        });
    }
    fn browse(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        self.task = None;
        self.busy = true;
        self.message = None;
        let (cancel, guard) = Cancellation::task();
        let work = cx
            .background_executor()
            .spawn(async move { session.list(&path, &cancel) });
        self.task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.busy = false;
                match result {
                    Ok((directory, entries)) => view.set_listing(directory, entries, window, cx),
                    Err(error) => view.message = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let path = self.selected.as_ref().unwrap_or(&self.directory).clone();
        self.task = None;
        self.busy = true;
        self.message = None;
        let reader = session.clone();
        let (cancel, guard) = Cancellation::task();
        let work = cx
            .background_executor()
            .spawn(async move { reader.list(&path, &cancel) });
        self.task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.busy = false;
                match result {
                    Ok((root, entries)) => {
                        let owner = view.owner.clone();
                        let add = view.add_folder;
                        if let Some(connection) = cx
                            .global_mut::<Connections>()
                            .catalog
                            .connections
                            .iter_mut()
                            .find(|item| item.id == session.connection.id)
                        {
                            connection.last_directory = Some(root.clone());
                        }
                        persist(cx);
                        window.close_dialog(cx);
                        let _ = owner.update(cx, |workspace, cx| {
                            workspace.receive_remote_folder(
                                session,
                                FolderSnapshot { root, entries },
                                add,
                                window,
                                cx,
                            )
                        });
                    }
                    Err(error) => view.message = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn delete(&mut self, connection: Connection, window: &mut Window, cx: &mut Context<Self>) {
        let used = crate::app::remote::connection_in_use(&connection.id, cx);
        if used {
            self.message = Some(tr(
                "Close workspaces using this connection before deleting it",
            ));
            cx.notify();
            return;
        }
        let prompt = window.prompt(
            PromptLevel::Warning,
            &tr("Delete SSH connection?"),
            Some(&tr(
                "Related recent workspaces will be removed. Remote files will not be deleted.",
            )),
            &[tr("Delete").as_str(), tr("Cancel").as_str()],
            cx,
        );
        self.task = Some(cx.spawn_in(window, async move |view, cx| {
            if prompt.await != Ok(0) {
                return;
            }
            let _ = view.update(cx, |view, cx| {
                if crate::app::remote::connection_in_use(&connection.id, cx) {
                    return;
                }
                let catalog = &mut cx.global_mut::<Connections>().catalog;
                let ids = catalog
                    .workspaces
                    .iter()
                    .filter(|(_, workspace)| workspace.connection == connection.id)
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>();
                catalog.connections.retain(|item| item.id != connection.id);
                for id in ids {
                    crate::app::remove_recent_workspace(id, cx);
                }
                // Keep source descriptors for closed, persisted windows; a removed
                // connection must never turn their remote paths into local paths.
                persist(cx);
                view.message = None;
                cx.notify();
            });
        }));
    }
    fn pick_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr("Choose Private Key").into()),
        });
        self.task = Some(cx.spawn_in(window, async move |view, cx| {
            if let Ok(Ok(Some(paths))) = selection.await
                && let Some(path) = paths.first()
            {
                let _ = view.update_in(cx, |view, window, cx| {
                    view.fields[4].update(cx, |field, cx| {
                        field.set_value(path.display().to_string(), window, cx)
                    })
                });
            }
        }));
    }
}

impl Render for RemoteDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let connections = cx.global::<Connections>().catalog.connections.clone();
        let mut body = v_flex().w_full().gap_4();
        match self.mode {
            Mode::Connections => {
                body = body.child(
                    h_flex()
                        .justify_between()
                        .child(tr("SSH Connections"))
                        .child(
                            Button::new("ssh-add")
                                .small()
                                .label(tr("Add SSH Connection…"))
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.open_after_save = false;
                                    view.edit(Connection::default(), window, cx);
                                })),
                        ),
                );
                let mut list = v_flex()
                    .id("ssh-connections-list")
                    .h(px(300.))
                    .overflow_y_scroll()
                    .gap_2();
                if connections.is_empty() {
                    list = list.child(
                        div()
                            .py_6()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr("No SSH connections yet")),
                    );
                }
                for connection in connections {
                    let open = connection.clone();
                    let edit = connection.clone();
                    let delete = connection.clone();
                    list =
                        list.child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .py_2()
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .child(div().truncate().child(connection.label()))
                                        .child(
                                            div()
                                                .truncate()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(connection.address()),
                                        ),
                                )
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "ssh-open-{}",
                                        connection.id
                                    )))
                                    .small()
                                    .label(tr("Open…"))
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        view.connect(open.clone(), window, cx)
                                    })),
                                )
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "ssh-edit-{}",
                                        connection.id
                                    )))
                                    .ghost()
                                    .small()
                                    .label(tr("Edit…"))
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        view.open_after_save = false;
                                        view.edit(edit.clone(), window, cx);
                                    })),
                                )
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "ssh-delete-{}",
                                        connection.id
                                    )))
                                    .ghost()
                                    .small()
                                    .label(tr("Delete"))
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        view.delete(delete.clone(), window, cx)
                                    })),
                                ),
                        );
                }
                body = body.child(list);
            }
            Mode::Edit => {
                for (index, label) in [
                    "Display Name (optional)",
                    "SSH Host",
                    "Username (optional)",
                    "SSH Port (optional)",
                ]
                .into_iter()
                .enumerate()
                {
                    body = body.child(
                        v_flex()
                            .gap_1()
                            .child(div().text_sm().child(tr(label)))
                            .child(Input::new(&self.fields[index]).disabled(self.busy)),
                    );
                }
                body = body.child(
                    h_flex()
                        .gap_3()
                        .child(
                            Switch::new("ssh-explicit-key")
                                .checked(self.explicit_key)
                                .disabled(self.busy)
                                .on_click(cx.listener(|view, checked: &bool, _, cx| {
                                    view.explicit_key = *checked;
                                    cx.notify();
                                })),
                        )
                        .child(tr("Use a specific private key")),
                );
                if self.explicit_key {
                    body =
                        body.child(
                            h_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .child(Input::new(&self.fields[4]).disabled(self.busy)),
                                )
                                .child(
                                    Button::new("ssh-key-file")
                                        .label(tr("Choose…"))
                                        .disabled(self.busy)
                                        .on_click(cx.listener(|view, _, window, cx| {
                                            view.pick_key(window, cx)
                                        })),
                                ),
                        );
                } else {
                    body = body.child(div().text_sm().text_color(cx.theme().muted_foreground).child(tr("Uses your Mac’s SSH configuration, agent and keys. Credentials are requested when needed.")));
                }
                body = body.child(
                    h_flex()
                        .justify_between()
                        .child(
                            Button::new("ssh-test")
                                .label(tr("Test Connection"))
                                .disabled(self.busy)
                                .on_click(
                                    cx.listener(|view, _, window, cx| view.save(true, window, cx)),
                                ),
                        )
                        .child(
                            Button::new("ssh-save")
                                .primary()
                                .label(tr(if self.open_after_save {
                                    "Save and Connect"
                                } else {
                                    "Save"
                                }))
                                .disabled(self.busy)
                                .on_click(
                                    cx.listener(|view, _, window, cx| view.save(false, window, cx)),
                                ),
                        ),
                );
            }
            Mode::Directories => {
                let weak = cx.entity().downgrade();
                let same_connection = self.add_folder.then(|| self.connection.id.clone());
                let selector = Button::new("ssh-location")
                    .label(self.connection.label())
                    .icon(IconName::ChevronDown)
                    .dropdown_menu(move |mut menu, _, _| {
                        for connection in &connections {
                            if same_connection
                                .as_ref()
                                .is_some_and(|id| *id != connection.id)
                            {
                                continue;
                            }
                            let connection = connection.clone();
                            let owner = weak.clone();
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
                body = body.child(selector).child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("ssh-parent")
                                .icon(IconName::ArrowUp)
                                .tooltip(tr("Parent Folder"))
                                .disabled(self.busy || self.directory.parent().is_none())
                                .on_click(cx.listener(|view, _, window, cx| {
                                    if let Some(parent) = view.directory.parent() {
                                        view.browse(parent.to_path_buf(), window, cx);
                                    }
                                })),
                        )
                        .child(
                            div()
                                .flex_1()
                                .child(Input::new(&self.path).disabled(self.busy)),
                        )
                        .child(
                            Button::new("ssh-refresh")
                                .icon(IconName::RotateCw)
                                .tooltip(tr("Refresh"))
                                .disabled(self.busy)
                                .on_click(cx.listener(|view, _, window, cx| {
                                    if view.session.is_none() || view.message.is_some() {
                                        view.connect(view.connection.clone(), window, cx);
                                    } else {
                                        view.browse(view.directory.clone(), window, cx);
                                    }
                                })),
                        ),
                );
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
                body = body.child(div().h(px(280.)).border_1().border_color(cx.theme().border).rounded(px(8.)).overflow_hidden()
                    .when(count == 0, |view| view.child(div().p_4().text_color(cx.theme().muted_foreground).child(tr(if self.busy { "Connecting…" } else { "No subfolders" }))))
                    .when(count > 0, |view| view.child(uniform_list("ssh-directories", count, cx.processor(move |view, range: std::ops::Range<usize>, _, cx| {
                        range.map(|index| {
                            let path = entries[index].path.clone();
                            let open = path.clone();
                            let selected = view.selected.as_ref() == Some(&path);
                            h_flex().id(("ssh-directory", index)).h(px(34.)).px_3().gap_2().items_center()
                                .when(selected, |row| row.bg(cx.theme().accent))
                                .hover(|row| row.bg(cx.theme().muted))
                                .child(Icon::new(IconName::Folder).size(px(16.)))
                                .child(div().flex_1().truncate().child(path.file_name().unwrap_or_default().to_string_lossy().into_owned()))
                                .on_mouse_down(MouseButton::Left, cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                                    if view.busy { return; }
                                    window.focus(&view.focus, cx);
                                    if event.click_count == 2 { view.browse(open.clone(), window, cx); }
                                    else { view.selected = Some(open.clone()); cx.notify(); }
                                })).into_any_element()
                        }).collect::<Vec<_>>()
                    })).track_scroll(&self.scroll).size_full())));
                body = body.child(
                    h_flex()
                        .gap_2()
                        .child(
                            Switch::new("ssh-hidden")
                                .small()
                                .checked(self.show_hidden)
                                .on_click(cx.listener(|view, checked: &bool, _, cx| {
                                    view.show_hidden = *checked;
                                    cx.notify();
                                })),
                        )
                        .child(div().text_sm().child(tr("Show hidden folders"))),
                );
                let target = self
                    .selected
                    .as_ref()
                    .unwrap_or(&self.directory)
                    .display()
                    .to_string();
                body = body.child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(tr("Folder to open")),
                                )
                                .child(div().truncate().text_sm().child(target)),
                        )
                        .child(
                            Button::new("ssh-open-folder")
                                .primary()
                                .label(tr("Open Folder"))
                                .disabled(self.busy || self.session.is_none())
                                .on_click(cx.listener(|view, _, window, cx| view.open(window, cx))),
                        ),
                );
            }
        }
        if let Some(message) = &self.message {
            body = body.child(div().text_sm().whitespace_normal().child(message.clone()));
        }
        if self.busy {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr("Connecting…")),
            );
        }
        body.track_focus(&self.focus)
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                if view.mode != Mode::Directories || view.busy || !view.focus.is_focused(window) {
                    return;
                }
                let paths = view
                    .entries
                    .iter()
                    .filter(|entry| {
                        entry.is_dir
                            && (view.show_hidden
                                || !entry
                                    .path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .starts_with('.'))
                    })
                    .map(|entry| entry.path.clone())
                    .collect::<Vec<_>>();
                let current = view
                    .selected
                    .as_ref()
                    .and_then(|selected| paths.iter().position(|path| path == selected));
                let next = match event.keystroke.key.as_str() {
                    "down" => Some(
                        current.map_or(0, |index| (index + 1).min(paths.len().saturating_sub(1))),
                    ),
                    "up" => Some(current.unwrap_or(0).saturating_sub(1)),
                    "enter" => {
                        if let Some(path) = view.selected.clone() {
                            view.browse(path, window, cx);
                        }
                        None
                    }
                    _ => None,
                };
                if let Some(index) = next
                    && let Some(path) = paths.get(index)
                {
                    view.selected = Some(path.clone());
                    view.scroll.scroll_to_item(index, ScrollStrategy::Top);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(
                h_flex().justify_end().child(
                    Button::new("ssh-cancel")
                        .ghost()
                        .label(tr("Cancel"))
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.task = None;
                            view.busy = false;
                            if view.mode == Mode::Edit && !view.open_after_save {
                                view.mode = Mode::Connections;
                                view.message = None;
                                cx.notify();
                            } else {
                                window.close_dialog(cx);
                            }
                        })),
                ),
            )
    }
}

use gpui_kit::assets::IconName;

impl Workspace {
    pub(crate) fn remote_locations(&self, cx: &App) -> Vec<Connection> {
        cx.global::<Connections>().catalog.connections.clone()
    }
    pub(crate) fn show_remote(
        &mut self,
        connection: Option<Connection>,
        manage: bool,
        add_folder: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = cx.entity().downgrade();
        let dialog = cx
            .new(|cx| RemoteDialog::new(owner, Mode::Connections, !manage, add_folder, window, cx));
        let content = dialog.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(tr("Remote Workspaces"))
                .width(px(560.))
                .child(content.clone())
        });
        dialog.update(cx, |view, cx| {
            if let Some(connection) = connection {
                view.connect(connection, window, cx);
            } else if !manage && cx.global::<Connections>().catalog.connections.is_empty() {
                view.edit(Connection::default(), window, cx);
            }
        });
    }
    pub(crate) fn add_remote_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owner = cx.entity().downgrade();
        let dialog = cx.new(|cx| RemoteDialog::new(owner, Mode::Edit, true, false, window, cx));
        dialog.update(cx, |view, cx| view.edit(Connection::default(), window, cx));
        window.open_dialog(cx, move |panel, _, _| {
            panel
                .title(tr("Add SSH Connection…"))
                .width(px(520.))
                .child(dialog.clone())
        });
    }
    fn receive_remote_folder(
        &mut self,
        session: Session,
        snapshot: FolderSnapshot,
        add: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if add {
            self.add_folder(snapshot, window, cx);
            self.remember_remote(cx);
            return;
        }
        let id = glim_services::remote::workspace_id(
            &session.connection.id,
            std::slice::from_ref(&snapshot.root),
        );
        if let Some(host) = self.workspace_window.clone() {
            let _ = host.update(cx, |host, cx| {
                if host.slots.iter().any(|slot| slot.workspace.id == id) {
                    host.select(&id, window, cx);
                    return;
                }
                let destination = host.append(None, None, window, cx);
                destination.update(cx, |view, cx| {
                    view.recent_id = id;
                    view.remote = Some(session);
                    view.page = super::Page::Document;
                    view.add_folder(snapshot, window, cx);
                    view.remember_remote(cx);
                    view.remember_current_workspace(cx);
                });
            });
        }
    }
    pub(super) fn remember_remote(&self, cx: &mut App) {
        if let Some(remote) = &self.remote {
            cx.global_mut::<Connections>().catalog.workspaces.insert(
                self.recent_id.clone(),
                RemoteWorkspace {
                    connection: remote.connection.id.clone(),
                    roots: self.roots.clone(),
                },
            );
            persist(cx);
        }
    }
    pub(super) fn remote_connection(&self, cx: &App) -> Option<Connection> {
        let catalog = &cx.try_global::<Connections>()?.catalog;
        let descriptor = catalog.workspaces.get(&self.recent_id)?;
        catalog
            .connections
            .iter()
            .find(|connection| connection.id == descriptor.connection)
            .cloned()
    }
    pub(super) fn open_remote_file(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.remote.clone() else {
            return;
        };
        let path = session.remote_path(&path).unwrap_or(path);
        if let Some(index) = self
            .tabs
            .iter()
            .position(|reader| reader.document.path == path)
        {
            self.activate_tab(index, window, cx);
            return;
        }
        self.read_remote_file(path, window, cx);
    }
    fn read_remote_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.remote.clone() else {
            return;
        };
        self.page = super::Page::Document;
        self.loading = true;
        self.error = None;
        session.received.store(0, Ordering::Relaxed);
        let transfer = session.clone();
        let remote_path = path.clone();
        let (cancel, guard) = Cancellation::task();
        let work = crate::app::reader_jobs::run(
            cx.background_executor().clone(),
            crate::app::reader_jobs::JobClass::Document,
            crate::app::reader_jobs::Priority::Interactive,
            cancel,
            move |cancel| transfer.read_content(&remote_path, cancel),
        );
        self.load_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.loading = false;
                view.remote_progress_task = None;
                match result {
                    Ok(content) => {
                        let mut reader =
                            crate::views::reader::Reader::from_content(content, window, cx);
                        reader.set_remote(session, path, cx);
                        if let Some(index) = view
                            .tabs
                            .iter()
                            .position(|existing| existing.document.path == reader.document.path)
                        {
                            view.tabs[index] = reader;
                            view.active = Some(index);
                        } else {
                            view.tabs.insert(0, reader);
                            view.active = Some(0);
                        }
                        view.remote_disconnected = false;
                        view.reveal_active_file(cx);
                        view.record_history(cx);
                    }
                    Err(error) => {
                        view.remote_disconnected = !session.is_connected();
                        view.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            });
        }));
        self.remote_progress_task = Some(cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(200))
                    .await;
                if view.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        }));
        cx.notify();
    }
    pub(super) fn restore_remote_workspace(
        &mut self,
        session: glim_core::session::WorkspaceSession,
        descriptor: RemoteWorkspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let connection = cx
            .global::<Connections>()
            .catalog
            .connections
            .iter()
            .find(|connection| connection.id == descriptor.connection)
            .cloned();
        self.remote_restore = Some(session.clone());
        self.roots = session.roots.clone();
        self.recent_id = session.workspace_id.clone().unwrap_or_default();
        self.workspace_name = session.name.clone();
        self.page = if session.show_home {
            super::Page::Start
        } else {
            super::Page::Document
        };
        self.remote_disconnected = true;
        let Some(connection) = connection else {
            self.error = Some(
                tr("SSH connection was removed. Open a folder using an available connection.")
                    .into(),
            );
            return;
        };
        self.loading = true;
        let (cancel, guard) = Cancellation::task();
        let worker_cancel = cancel.clone();
        let work = cx.background_executor().spawn(async move {
            let remote = Session::connect(connection, &worker_cancel)?;
            let mut folders = Vec::new();
            for root in descriptor.roots {
                let (root, entries) = remote.list(&root, &worker_cancel)?;
                folders.push(FolderSnapshot { root, entries });
            }
            anyhow::Ok((remote, folders))
        });
        self.load_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let (remote, folders) = match work.await {
                Ok(result) => result,
                Err(error) => {
                    let _ = view.update(cx, |view, cx| {
                        view.loading = false;
                        view.error = Some(error.to_string().into());
                        cx.notify();
                    });
                    return;
                }
            };
            let _ = view.update_in(cx, |view, window, cx| {
                view.remote = Some(remote.clone());
                view.remote_disconnected = false;
                view.error = None;
                view.roots.clear();
                view.explorer = None;
                view.subscriptions.clear();
                for folder in folders {
                    view.add_folder(folder, window, cx);
                }
                cx.notify();
            });
            let mut restored = Vec::new();
            for tab in &session.tabs {
                let path = match tab {
                    glim_core::session::SessionTab::File { path, .. }
                    | glim_core::session::SessionTab::Playlist { path, .. } => path.clone(),
                };
                let transfer = remote.clone();
                let requested = path.clone();
                let content = crate::app::reader_jobs::run(
                    cx.background_executor().clone(),
                    crate::app::reader_jobs::JobClass::Document,
                    crate::app::reader_jobs::Priority::Background,
                    cancel.clone(),
                    move |cancel| transfer.read_content(&requested, cancel),
                )
                .await;
                let remote = remote.clone();
                let preview = matches!(
                    tab,
                    glim_core::session::SessionTab::File { preview: true, .. }
                );
                if view
                    .update_in(cx, |view, window, cx| {
                        match content {
                            Ok(content) => {
                                let mut reader =
                                    crate::views::reader::Reader::from_restored_content(
                                        content, window, cx,
                                    );
                                reader.set_remote(remote, path, cx);
                                reader.set_preview(preview, cx);
                                restored.push(reader);
                            }
                            Err(error) => {
                                view.remote_disconnected = !remote.is_connected();
                                let mut reader = crate::views::reader::Reader::unavailable(
                                    path.clone(),
                                    (&error).into(),
                                    window,
                                    cx,
                                );
                                reader.set_remote(remote, path, cx);
                                reader.set_preview(preview, cx);
                                restored.push(reader);
                                view.error = Some(error.to_string().into());
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
            let _ = view.update_in(cx, |view, window, cx| {
                view.remote_restore = None;
                view.tabs = restored;
                view.active = session
                    .active
                    .filter(|index| *index < view.tabs.len())
                    .or_else(|| (!view.tabs.is_empty()).then_some(0));
                view.loading = false;
                view.sync_workspace(cx);
                view.remember_session(cx);
                view.reveal_active_file(cx);
                if let Some(index) = view.active {
                    view.tabs[index].focus_source(window, cx);
                }
                cx.notify();
            });
        }));
    }
    pub(super) fn reconnect_remote(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let descriptor = cx
            .global::<Connections>()
            .catalog
            .workspaces
            .get(&self.recent_id)
            .cloned();
        if let Some(descriptor) = descriptor {
            let session = self.update_session(cx);
            self.restore_remote_workspace(session, descriptor, window, cx);
        }
    }
    pub(super) fn refresh_remote(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.remote_disconnected || self.remote_restore.is_some() {
            self.reconnect_remote(window, cx);
            return;
        }
        if let Some(explorer) = &self.explorer {
            explorer.update(cx, |view, cx| view.refresh(cx));
        }
        if let Some(reader) = self.active_reader() {
            self.read_remote_file(reader.document.path.clone(), window, cx);
        }
    }
}
