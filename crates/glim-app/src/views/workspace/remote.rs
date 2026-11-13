mod dialog;
mod watch;

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
        ActiveTheme, Disableable, Icon, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        input::{Input, InputEvent, InputState},
        menu::{DropdownMenu, PopupMenuItem},
        spinner::Spinner,
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

pub(super) struct RemoteDialog {
    owner: WeakEntity<Workspace>,
    mode: Mode,
    connection: Connection,
    session: Option<Session>,
    tested_session: Option<Session>,
    fields: Vec<Entity<InputState>>,
    path: Entity<InputState>,
    entries: Vec<glim_core::DirectoryEntry>,
    selected: Option<PathBuf>,
    directory: PathBuf,
    busy: bool,
    activity: &'static str,
    message: Option<String>,
    message_success: bool,
    task: Option<Task<()>>,
    feedback_task: Option<Task<()>>,
    modal_focus: FocusHandle,
    _path_subscription: Subscription,
    _field_subscriptions: Vec<Subscription>,
    _workspace_subscriptions: Vec<Subscription>,
    open_after_save: bool,
    add_folder: bool,
    connections_scroll: ScrollHandle,
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
        let subscription =
            cx.subscribe_in(&path, window, |view, _, event, window, cx| match event {
                InputEvent::Change => {
                    view.selected = None;
                    cx.notify();
                }
                InputEvent::Focus | InputEvent::Blur => cx.notify(),
                InputEvent::PressEnter { .. } if !view.busy => {
                    let path = PathBuf::from(view.path.read(cx).value().trim());
                    view.browse(path, window, cx);
                }
                _ => {}
            });
        let fields: Vec<Entity<InputState>> = (0..5)
            .map(|_| cx.new(|cx| InputState::new(window, cx)))
            .collect();
        let field_subscriptions = fields
            .iter()
            .map(|field| {
                cx.subscribe_in(field, window, |view, _, event, window, cx| match event {
                    InputEvent::Change => {
                        view.message = None;
                        view.message_success = false;
                        cx.notify();
                    }
                    InputEvent::Focus | InputEvent::Blur => cx.notify(),
                    InputEvent::PressEnter { .. } if !view.busy => view.save(false, window, cx),
                    _ => {}
                })
            })
            .collect();
        let workspace_subscriptions = crate::app::remote::observe_connection_status(cx);
        Self {
            owner,
            mode,
            connection: Connection::default(),
            session: None,
            tested_session: None,
            fields,
            path,
            entries: Vec::new(),
            selected: None,
            directory: PathBuf::new(),
            busy: false,
            activity: "Connecting…",
            message: None,
            message_success: false,
            task: None,
            feedback_task: None,
            modal_focus: cx.focus_handle(),
            _path_subscription: subscription,
            _field_subscriptions: field_subscriptions,
            _workspace_subscriptions: workspace_subscriptions,
            open_after_save,
            add_folder,
            connections_scroll: ScrollHandle::new(),
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle().tab_stop(true),
        }
    }
    fn success(&mut self, message: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.message_success = true;
        self.message = Some(tr(message));
        self.feedback_task = Some(cx.spawn_in(window, async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            let _ = view.update(cx, |view, cx| {
                if view.message_success {
                    view.message = None;
                    view.message_success = false;
                    cx.notify();
                }
            });
        }));
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.task = None;
        self.feedback_task = None;
        let _ = self.owner.update(cx, |workspace, cx| {
            workspace.remote_dialog = None;
            window.focus(&workspace.focus, cx);
            cx.notify();
        });
    }
    fn edit(&mut self, connection: Connection, window: &mut Window, cx: &mut Context<Self>) {
        self.task = None;
        self.feedback_task = None;
        self.busy = false;
        self.message = None;
        self.message_success = false;
        self.mode = Mode::Edit;
        self.tested_session = None;
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
        self.fields[0].update(cx, |field, cx| field.focus(window, cx));
        cx.notify();
    }
    fn form_connection(&self, cx: &App) -> anyhow::Result<Connection> {
        let value = |index: usize| self.fields[index].read(cx).value().trim().to_owned();
        let port = value(3);
        let identity = value(4);
        let identity = if !identity.is_empty() {
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
    fn resolve_directory(&self, path: PathBuf) -> PathBuf {
        let Some(session) = &self.session else {
            return path;
        };
        if path == std::path::Path::new("~") {
            return session.home.clone();
        }
        if let Ok(relative) = path.strip_prefix("~") {
            return session.home.join(relative);
        }
        if path.is_relative() {
            return self.directory.join(path);
        }
        path
    }
    fn target_directory(&self, cx: &App) -> PathBuf {
        self.selected.clone().unwrap_or_else(|| {
            self.resolve_directory(PathBuf::from(self.path.read(cx).value().trim()))
        })
    }
    fn save(&mut self, test: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.feedback_task = None;
        window.focus(&self.modal_focus, cx);
        self.message_success = false;
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
        self.activity = if test {
            "Testing connection…"
        } else {
            "Saving connection…"
        };
        self.message = None;
        let (cancel, guard) = Cancellation::task();
        let tested_session = self.tested_session.take();
        let started = std::time::Instant::now();
        let work = cx.background_executor().spawn(async move {
            connection.validate()?;
            let session = if test {
                let session = Session::connect(connection.clone(), &cancel)?;
                session.list(&session.home, &cancel)?;
                Some(session)
            } else {
                tested_session
                    .filter(|session| crate::app::remote::session_matches(session, &connection))
                    .map(|mut session| {
                        session.connection = connection.clone();
                        session
                    })
            };
            anyhow::Ok((connection, session))
        });
        self.task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result_with(
                "ssh_connection",
                None,
                started,
                &result,
                [(
                    "action",
                    glim_services::telemetry::text(if test { "test" } else { "save" }),
                )],
            );
            let _ = view.update_in(cx, |view, window, cx| {
                view.busy = false;
                match result {
                    Err(error) => view.message = Some(tr(&error.to_string())),
                    Ok((_, session)) if test => {
                        view.tested_session = session;
                        view.success("Connection successful", window, cx);
                    }
                    Ok((connection, session)) => {
                        let state = cx.global_mut::<Connections>();
                        if let Some(session) = session {
                            state.sessions.insert(connection.id.clone(), session);
                        } else if state.sessions.get(&connection.id).is_some_and(|session| {
                            !crate::app::remote::session_matches(session, &connection)
                        }) {
                            state.sessions.remove(&connection.id);
                        }
                        view.tested_session = None;
                        let catalog = &mut state.catalog;
                        if let Some(existing) = catalog
                            .connections
                            .iter_mut()
                            .find(|item| item.id == connection.id)
                        {
                            *existing = connection.clone();
                        } else {
                            catalog.connections.insert(0, connection.clone());
                            view.connections_scroll.set_offset(point(px(0.), px(0.)));
                        }
                        persist(cx);
                        if view.open_after_save {
                            view.connect(connection, window, cx);
                        } else {
                            view.mode = Mode::Connections;
                            view.message = None;
                            window.focus(&view.modal_focus, cx);
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
        self.feedback_task = None;
        window.focus(&self.modal_focus, cx);
        let cached = crate::app::remote::cached_session(&connection, cx);
        self.session = None;
        self.mode = Mode::Directories;
        self.connection = connection.clone();
        self.entries.clear();
        self.directory = connection.last_directory.clone().unwrap_or_default();
        self.path.update(cx, |path, cx| {
            path.set_value(self.directory.display().to_string(), window, cx)
        });
        self.selected = None;
        self.message = None;
        self.message_success = false;
        self.busy = true;
        self.activity = "Connecting…";
        let started = std::time::Instant::now();
        let (cancel, guard) = Cancellation::task();
        let work = cx.background_executor().spawn(async move {
            let session = match cached {
                Some(session) => session,
                None => Session::connect(connection.clone(), &cancel)?,
            };
            let path = connection.last_directory.as_ref().unwrap_or(&session.home);
            let listing = session
                .list(path, &cancel)
                .or_else(|_| session.list(&session.home, &cancel))?;
            anyhow::Ok((session, listing))
        });
        self.task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result_with(
                "ssh_connection",
                None,
                started,
                &result,
                [("action", glim_services::telemetry::text("connect"))],
            );
            let _ = view.update_in(cx, |view, window, cx| {
                view.busy = false;
                match result {
                    Ok((session, (directory, entries))) => {
                        cx.global_mut::<Connections>()
                            .sessions
                            .insert(session.connection.id.clone(), session.clone());
                        view.session = Some(session);
                        view.set_listing(directory, entries, window, cx);
                        cx.refresh_windows();
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
        if self.busy {
            return;
        }
        let path = self.resolve_directory(path);
        let Some(session) = self.session.clone() else {
            return;
        };
        self.task = None;
        self.busy = true;
        self.activity = "Loading folders…";
        self.message = None;
        self.message_success = false;
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
        if self.busy {
            return;
        }
        let Some(session) = self.session.clone() else {
            return;
        };
        let path = self.target_directory(cx);
        self.task = None;
        self.busy = true;
        self.activity = "Opening folder…";
        self.message = None;
        self.message_success = false;
        let reader = session.clone();
        let started = std::time::Instant::now();
        let (cancel, guard) = Cancellation::task();
        let work = cx
            .background_executor()
            .spawn(async move { reader.list(&path, &cancel) });
        self.task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result_with(
                "workspace_open",
                None,
                started,
                &result,
                [
                    ("storage", glim_services::telemetry::text("ssh")),
                    ("trigger", glim_services::telemetry::text("open")),
                ],
            );
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
                        view.close(window, cx);
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
        self.message_success = false;
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
            let _ = view.update_in(cx, |view, window, cx| {
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
                cx.global_mut::<Connections>()
                    .sessions
                    .remove(&connection.id);
                if view
                    .session
                    .as_ref()
                    .is_some_and(|session| session.connection.id == connection.id)
                {
                    view.session = None;
                }
                for id in ids {
                    crate::app::remove_recent_workspace(id, cx);
                }
                // Keep source descriptors for closed, persisted windows; a removed
                // connection must never turn their remote paths into local paths.
                persist(cx);
                glim_services::telemetry::track(
                    "ssh_connection",
                    [
                        ("action", glim_services::telemetry::text("delete")),
                        ("result", glim_services::telemetry::text("success")),
                    ],
                );
                view.message = None;
                window.focus(&view.modal_focus, cx);
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

use gpui_kit::assets::IconName;

impl Workspace {
    pub(crate) fn open_external_remote(
        &mut self,
        path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = path
            && !self.remote_open_paths.contains(&path)
        {
            self.remote_open_paths.push_back(path);
        }
        if self.loading {
            return;
        }
        if !self.remote_workspace_is_connected() || self.remote_restore.is_some() {
            self.reconnect_remote(window, cx);
        } else {
            self.drain_remote_open_paths(window, cx);
        }
    }

    pub(super) fn drain_remote_open_paths(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.remote_restore.is_some() || !self.remote_workspace_is_connected() {
            return;
        }
        while !self.loading {
            let Some(path) = self.remote_open_paths.pop_front() else {
                break;
            };
            self.open_remote_file(path, window, cx);
        }
    }

    pub(crate) fn remote_workspace_is_connected(&self) -> bool {
        self.remote.as_ref().is_some_and(Session::is_connected)
    }
    pub(crate) fn remote_session(&self) -> Option<Session> {
        self.remote.clone()
    }
    pub(crate) fn remote_connection_is_live(&self, connection: &Connection) -> bool {
        self.remote.as_ref().is_some_and(|session| {
            session.connection.id == connection.id
                && crate::app::remote::session_matches(session, connection)
        })
    }
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
        self.close_search(window, cx);
        let owner = cx.entity().downgrade();
        let dialog = cx
            .new(|cx| RemoteDialog::new(owner, Mode::Connections, !manage, add_folder, window, cx));
        self.remote_dialog = Some(dialog.clone());
        let modal_focus = dialog.read(cx).modal_focus.clone();
        window.focus(&modal_focus, cx);
        dialog.update(cx, |view, cx| {
            if let Some(connection) = connection {
                view.connect(connection, window, cx);
            } else if !manage && cx.global::<Connections>().catalog.connections.is_empty() {
                view.edit(Connection::default(), window, cx);
            }
        });
        cx.notify();
    }
    pub(crate) fn add_remote_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owner = cx.entity().downgrade();
        let dialog = cx.new(|cx| RemoteDialog::new(owner, Mode::Edit, true, false, window, cx));
        dialog.update(cx, |view, cx| view.edit(Connection::default(), window, cx));
        self.remote_dialog = Some(dialog);
        cx.notify();
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
            // Appending suspends and reads the current workspace. Release its
            // entity update before asking the window host to switch workspaces.
            window.defer(cx, move |window, cx| {
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
                        view.remember_current_workspace(cx);
                    });
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
        if self.pending_open_path() == Some(&path) {
            return;
        }
        if let Some(index) = self
            .tabs
            .iter()
            .position(|reader| reader.document.path == path)
        {
            self.activate_tab(index, window, cx);
            return;
        }
        self.read_remote_file(path, false, window, cx);
    }
    fn read_remote_file(
        &mut self,
        path: PathBuf,
        refresh: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.remote.clone() else {
            return;
        };
        self.page = super::Page::Document;
        self.loading = true;
        self.error = None;
        self.loading_path = Some(path.clone());
        self.tab_scroll.scroll_to_item(
            self.tabs
                .iter()
                .position(|reader| reader.document.path == path)
                .unwrap_or(0),
        );
        session.received.store(0, Ordering::Relaxed);
        let transfer = session.clone();
        let remote_path = path.clone();
        if !refresh {
            glim_services::telemetry::file_event(
                "file_open_requested",
                &path,
                [("storage", glim_services::telemetry::text("ssh"))],
            );
        }
        let started = std::time::Instant::now();
        let (cancel, guard) = Cancellation::task();
        let work = crate::app::reader_jobs::run(
            cx.background_executor().clone(),
            crate::app::reader_jobs::JobClass::Document,
            crate::app::reader_jobs::Priority::Interactive,
            cancel,
            move |cancel| {
                let result = transfer.read_content(&remote_path, cancel);
                if cancel.check().is_ok() {
                    if refresh {
                        if let Err(error) = &result {
                            glim_services::telemetry::operation_failed(
                                "remote_file_refresh",
                                Some(&remote_path),
                                error,
                            );
                        }
                    } else {
                        glim_services::telemetry::result_with(
                            "file_open",
                            Some(&remote_path),
                            started,
                            &result,
                            [
                                ("storage", glim_services::telemetry::text("ssh")),
                                ("trigger", glim_services::telemetry::text("open")),
                            ],
                        );
                    }
                }
                result
            },
        );
        self.load_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.loading = false;
                match result {
                    Ok(content) => {
                        let mut reader =
                            crate::views::reader::Reader::from_content(content, window, cx);
                        reader.set_remote(session, path, cx);
                        if !refresh {
                            reader.report_opened("open");
                        }
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
                        view.reveal_search_location(window, cx);
                        view.reveal_active_file(cx);
                        view.record_history(cx);
                    }
                    Err(error) => {
                        if error.is::<glim_services::work::Cancelled>() {
                            view.loading_path = None;
                            cx.notify();
                            return;
                        }
                        view.remote_disconnected = !session.is_connected();
                        let existing = view
                            .tabs
                            .iter()
                            .position(|reader| reader.document.path == path);
                        if refresh
                            && existing.is_some_and(|index| !view.tabs[index].is_unavailable())
                        {
                            // Preserve a readable snapshot when refreshing fails.
                            view.error = Some(error.to_string().into());
                        } else {
                            let mut reader = crate::views::reader::Reader::unavailable(
                                path.clone(),
                                (&error).into(),
                                window,
                                cx,
                            );
                            reader.set_remote(session, path, cx);
                            if !refresh {
                                reader.report_opened("open");
                            }
                            if let Some(index) = existing {
                                view.tabs[index] = reader;
                                view.active = Some(index);
                            } else {
                                view.tabs.insert(0, reader);
                                view.active = Some(0);
                            }
                            view.reveal_active_file(cx);
                            view.record_history(cx);
                        }
                    }
                }
                view.loading_path = None;
                view.drain_remote_open_paths(window, cx);
                cx.notify();
            });
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
        self.loading = false;
        self.loading_path = None;
        let Some(connection) = connection else {
            self.error = Some(
                tr("SSH connection was removed. Open a folder using an available connection.")
                    .into(),
            );
            return;
        };
        self.loading = true;
        cx.refresh_windows();
        let (cancel, guard) = Cancellation::task();
        let worker_cancel = cancel.clone();
        let started = std::time::Instant::now();
        let cached = crate::app::remote::cached_session(&connection, cx);
        let work = cx.background_executor().spawn(async move {
            let remote = match cached {
                Some(remote) => remote,
                None => Session::connect(connection, &worker_cancel)?,
            };
            let mut folders = Vec::new();
            for root in descriptor.roots {
                let (root, entries) = remote.list(&root, &worker_cancel)?;
                folders.push(FolderSnapshot { root, entries });
            }
            anyhow::Ok((remote, folders))
        });
        self.load_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result_with(
                "workspace_open",
                None,
                started,
                &result,
                [
                    ("storage", glim_services::telemetry::text("ssh")),
                    ("trigger", glim_services::telemetry::text("restore")),
                ],
            );
            let (remote, folders) = match result {
                Ok(result) => result,
                Err(error) => {
                    let _ = view.update(cx, |view, cx| {
                        view.loading = false;
                        view.error = Some(error.to_string().into());
                        cx.refresh_windows();
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
                                reader.report_opened("restore");
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
                                reader.report_opened("restore");
                                restored.push(reader);
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
                cx.refresh_windows();
                view.sync_workspace(cx);
                view.remember_session(cx);
                view.reveal_active_file(cx);
                if view.is_visible_workspace(cx)
                    && let Some(index) = view.active
                {
                    view.tabs[index].focus_source(window, cx);
                }
                view.drain_remote_open_paths(window, cx);
                cx.notify();
            });
        }));
    }
    pub(crate) fn resume_remote_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || !glim_services::remote::is_remote_id(&self.recent_id) {
            return;
        }
        if self.remote_restore.is_some() || self.remote.is_none() {
            self.reconnect_remote(window, cx);
            return;
        }
        let Some(remote) = self.remote.clone() else {
            return;
        };
        let was_disconnected = self.remote_disconnected;
        self.remote_disconnected = !remote.is_connected();
        if !self.remote_disconnected {
            if was_disconnected {
                self.error = None;
                if let Some(explorer) = &self.explorer {
                    explorer.update(cx, |explorer, cx| explorer.refresh(cx));
                }
                cx.notify();
            }
            return;
        }
        self.loading = true;
        self.loading_path = None;
        self.error = None;
        let path = self.roots.first().cloned().unwrap_or(remote.home.clone());
        let (cancel, guard) = Cancellation::task();
        // Reusing the transport keeps existing readers and cached files alive.
        let work = cx.background_executor().spawn(async move {
            let result = remote.list_entries(&path, &cancel);
            (remote, path, result)
        });
        self.load_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let (remote, path, result) = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.loading = false;
                view.remote_disconnected = !remote.is_connected();
                match result {
                    Ok(entries) => {
                        if let Some(explorer) = &view.explorer {
                            explorer.update(cx, |explorer, cx| {
                                explorer.apply_remote_listing(path, entries, cx);
                                explorer.refresh(cx);
                            });
                        }
                        view.drain_remote_open_paths(window, cx);
                    }
                    Err(error) => view.error = Some(error.to_string().into()),
                }
                cx.refresh_windows();
                cx.notify();
            });
        }));
        cx.notify();
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
        if self.remote_disconnected
            || self.remote_restore.is_some()
            || self
                .remote
                .as_ref()
                .is_some_and(|session| !session.is_connected())
        {
            self.reconnect_remote(window, cx);
            return;
        }
        if let Some(explorer) = &self.explorer {
            explorer.update(cx, |view, cx| view.refresh(cx));
        }
        if let Some(reader) = self.active_reader() {
            self.read_remote_file(reader.document.path.clone(), true, window, cx);
        }
    }
}
