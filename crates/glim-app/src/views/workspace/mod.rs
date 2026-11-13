mod editing;
mod file_operations;
mod gestures;
mod header;
mod layout;
mod loading;
mod moving;
mod reader_actions;
mod remote;
mod render;
mod search;
mod session;
mod settings;
mod tabs;
mod toolbar;
mod updates;
mod usage;
mod workspaces;

pub(crate) const TITLE_BAR_HEIGHT: f32 = 40.;
pub(crate) const CONTENT_INSET: f32 = 6.;
const WORKSPACE_ROW_HEIGHT: f32 = 40.;
const COLLAPSED_WORKSPACE_WIDTH: f32 = 56.;
const COLLAPSED_WORKSPACE_ROW_WIDTH: f32 = 48.;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use std::path::PathBuf;

use super::{explorer::Explorer, reader::Reader};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Document,
    Start,
    Settings,
}

const DEFAULT_SIDEBAR_RATIO: f32 = 0.13;
const DEFAULT_EXPLORER_RATIO: f32 = 0.18;
const SIDEBAR_COLLAPSE_WIDTH: f32 = 120.;

pub struct Workspace {
    integrations: Entity<super::integrations::Integrations>,
    remote: Option<glim_services::remote::Session>,
    remote_dialog: Option<Entity<remote::RemoteDialog>>,
    remote_disconnected: bool,
    remote_restore: Option<glim_core::session::WorkspaceSession>,
    remote_open_paths: std::collections::VecDeque<PathBuf>,
    pub(crate) workspace_window: Option<WeakEntity<super::workspace_window::WorkspaceWindow>>,
    workspace_name: Option<String>,
    workspace_rename: Option<(String, Entity<InputState>, Subscription)>,
    path_hint: super::path_hint::PathHint,
    recent_id: String,
    restoring_roots: Option<Vec<PathBuf>>,
    focus: FocusHandle,
    page: Page,
    file_fullscreen: bool,
    reader_toolbar_collapsed: bool,
    reader_toolbar_expanded: bool,
    toolbar_feedback: Option<&'static str>,
    toolbar_feedback_task: Option<Task<()>>,
    reader_toolbar: std::rc::Rc<super::native_toolbar_bridge::NativeToolbar>,
    reader_toolbar_task: Option<Task<()>>,
    file_fullscreen_entered_window: bool,
    file_fullscreen_seen_window: bool,
    history: Vec<header::Target>,
    history_cursor: Option<usize>,
    navigating: bool,
    history_swipe: Option<gestures::HistorySwipe>,
    search: Entity<InputState>,
    search_visible: bool,
    search_filename_fallback: bool,
    content_results: Vec<glim_services::content_search::Match>,
    pending_search_location: Option<(PathBuf, u64)>,
    search_focus: FocusHandle,
    search_previous_focus: Option<FocusHandle>,
    search_selected: usize,
    search_scroll: ScrollHandle,
    search_loading: bool,
    search_error: Option<SharedString>,
    search_partial: bool,
    search_indexing: bool,
    search_indexed_files: Option<u64>,
    search_index_ready: bool,
    search_index_error: Option<SharedString>,
    remote_index_progress_task: Option<Task<()>>,
    remote_index_scope: Option<String>,
    remote_index_task: Option<Task<()>>,
    sidebar_visible: bool,
    sidebar_ratio: f32,
    sidebar_window_width: f32,
    sidebar_layout: Entity<gpui_kit::component::resizable::ResizableState>,
    explorer_visible: bool,
    explorer_ratio: f32,
    explorer_layout: Entity<gpui_kit::component::resizable::ResizableState>,
    explorer_container_width: f32,
    word_wrap: bool,
    preference_task: Option<Task<()>>,
    watch_scan_task: Option<Task<()>>,
    results: Vec<PathBuf>,
    search_offset: usize,
    search_more: bool,
    recent_visible: usize,
    pending_anchor: Option<(PathBuf, String)>,
    search_task: Option<Task<()>>,
    _search_subscription: Subscription,
    _appearance_subscription: Subscription,
    watch: Option<glim_services::watch::WorkspaceWatch>,
    watch_task: Option<Task<()>>,
    watch_setup: Option<Task<()>>,
    refresh_task: Option<Task<()>>,
    pending_refresh: glim_services::watch::Changes,
    force_refresh: bool,
    root: Option<PathBuf>,
    roots: Vec<PathBuf>,
    explorer: Option<Entity<Explorer>>,
    git_repositories: Vec<PathBuf>,
    git_discovery_task: Option<Task<()>>,
    changes_expanded: bool,
    subscriptions: Vec<Subscription>,
    tabs: Vec<Reader>,
    active: Option<usize>,
    tab_scroll: ScrollHandle,
    error: Option<SharedString>,
    refresh_notice: Option<(SharedString, SharedString)>,
    loading: bool,
    loading_path: Option<PathBuf>,
    load_task: Option<Task<()>>,
    incoming_files: std::collections::VecDeque<PathBuf>,
    incoming_file_task: Option<Task<()>>,
    picker_task: Option<Task<()>>,
    picking: bool,
    save_task: Option<Task<()>>,
    save_cancel: Option<glim_services::work::CancelOnDrop>,
    saving: Option<EntityId>,
    editing_locked: bool,
    usage: Option<usage::Segment>,
    scroll_usage: Option<usage::ScrollUsage>,
    scroll_usage_task: Option<Task<()>>,
    usage_task: Option<Task<()>>,
    usage_activation: Option<Subscription>,
    telemetry_edited: std::collections::HashSet<EntityId>,
    autosave: std::collections::HashMap<EntityId, (Subscription, Option<Task<()>>)>,
    save_queue: std::collections::VecDeque<EntityId>,
    confirm_task: Option<Task<()>>,
    close_request: Option<editing::CloseRequest>,
    recovery_keys: std::collections::HashMap<EntityId, String>,
    edit_revision: u64,
    quit_approval: Option<u64>,
    quit_discards: Vec<EntityId>,
    context_menu_task: Option<Task<()>>,
}

impl Workspace {
    pub(crate) fn set_editing_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        self.editing_locked = locked;
        if locked {
            self.refresh_task = None;
        }
        for reader in &mut self.tabs {
            reader.set_editing_locked(locked, cx);
        }
        cx.notify();
    }
    pub(crate) fn unlock_and_refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_editing_locked(false, cx);
        self.load_incoming_file(window, cx);
        self.refresh(window, cx);
        // Finish removing trashed readers before resuming paused autosaves.
        let view = cx.entity().downgrade();
        window.defer(cx, move |window, cx| {
            let _ = view.update(cx, |view, cx| view.resume_queued_saves(window, cx));
        });
    }

    pub(crate) fn recent_visible(&self) -> usize {
        self.recent_visible
    }
    pub(crate) fn show_more_recent(&mut self, cx: &mut Context<Self>) {
        self.recent_visible = self
            .recent_visible
            .saturating_add(glim_services::limits::WORKSPACE_RECENT_WORKSPACES);
        cx.notify();
    }

    pub(crate) fn matches_recent_workspace(
        &self,
        workspace: &glim_services::preferences::RecentWorkspace,
    ) -> bool {
        self.recent_id == workspace.id
            || (!glim_services::remote::is_remote_id(&workspace.id)
                && !glim_services::remote::is_remote_id(&self.recent_id)
                && !workspace.folders.is_empty()
                && (self.roots == workspace.folders
                    || self.restoring_roots.as_ref() == Some(&workspace.folders)))
    }

    pub(crate) fn show_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_search(window, cx);
        glim_services::telemetry::track("settings_opened", []);
        self.page = Page::Settings;
        self.integrations.update(cx, |view, cx| view.reload(cx));
        self.results.clear();
        self.search_task = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn refresh_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |search, cx| {
            search.set_placeholder(
                glim_services::locale::tr("Search files in workspace…"),
                window,
                cx,
            )
        });
        cx.notify();
    }

    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let toolbar_events = std::rc::Rc::new(std::cell::RefCell::new(
            super::native_toolbar_bridge::ToolbarEvents::default(),
        ));
        let reader_toolbar = std::rc::Rc::new(super::native_toolbar_bridge::NativeToolbar::new(
            toolbar_events.clone(),
            window,
        ));
        cx.on_release(|view, _| {
            view.flush_usage();
            glim_services::telemetry::track(
                "window_closed",
                [
                    (
                        "open_tabs",
                        glim_services::telemetry::number(view.tabs.len() as u64),
                    ),
                    (
                        "workspace_folders",
                        glim_services::telemetry::number(view.roots.len() as u64),
                    ),
                ],
            );
        })
        .detach();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(glim_services::locale::tr("Search files in workspace…"))
        });
        let subscription = cx.subscribe_in(&search, window, |view, _, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                view.search_input_changed(window, cx);
            }
        });
        let appearance_subscription = window.observe_window_appearance(|window, cx| {
            crate::app::sync_system_appearance(window, cx);
        });
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            if crate::app::updates::intercept_last_window_close(cx) {
                return false;
            }
            weak.update(cx, |v, cx| {
                if v.has_unsaved() || v.is_saving() {
                    v.request_close_window(window, cx);
                    false
                } else {
                    true
                }
            })
            .unwrap_or(true)
        });
        let mut workspace = Self {
            integrations: cx.new(|_| super::integrations::Integrations::default()),
            remote: None,
            remote_dialog: None,
            remote_disconnected: false,
            remote_restore: None,
            remote_open_paths: Default::default(),
            workspace_window: None,
            workspace_name: None,
            workspace_rename: None,
            path_hint: Default::default(),
            recent_id: format!(
                "{}-{:?}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                cx.entity_id(),
            ),
            restoring_roots: None,
            history: Vec::new(),
            history_cursor: None,
            navigating: false,
            history_swipe: None,
            sidebar_visible: true,
            sidebar_ratio: DEFAULT_SIDEBAR_RATIO,
            sidebar_window_width: 0.,
            sidebar_layout: cx.new(|_| gpui_kit::component::resizable::ResizableState::default()),
            explorer_visible: true,
            explorer_ratio: DEFAULT_EXPLORER_RATIO,
            explorer_layout: cx.new(|_| gpui_kit::component::resizable::ResizableState::default()),
            explorer_container_width: 0.,
            git_repositories: Vec::new(),
            git_discovery_task: None,
            changes_expanded: false,
            page: Page::Start,
            file_fullscreen: false,
            reader_toolbar_collapsed: false,
            reader_toolbar_expanded: false,
            toolbar_feedback: None,
            toolbar_feedback_task: None,
            reader_toolbar,
            reader_toolbar_task: None,
            file_fullscreen_entered_window: false,
            file_fullscreen_seen_window: false,
            word_wrap: cx
                .try_global::<crate::app::ReaderPreferences>()
                .is_some_and(|p| p.word_wrap),
            preference_task: None,
            watch_scan_task: None,
            search,
            search_visible: false,
            search_filename_fallback: false,
            content_results: Vec::new(),
            pending_search_location: None,
            search_focus: cx.focus_handle(),
            search_previous_focus: None,
            search_selected: 0,
            search_scroll: ScrollHandle::new(),
            search_loading: false,
            search_error: None,
            search_partial: false,
            search_indexing: false,
            search_indexed_files: None,
            search_index_ready: false,
            search_index_error: None,
            remote_index_progress_task: None,
            remote_index_scope: None,
            remote_index_task: None,
            results: Vec::new(),
            search_offset: 0,
            search_more: false,
            recent_visible: glim_services::limits::WORKSPACE_RECENT_WORKSPACES,
            pending_anchor: None,
            search_task: None,
            _search_subscription: subscription,
            _appearance_subscription: appearance_subscription,
            watch: None,
            watch_task: None,
            watch_setup: None,
            refresh_task: None,
            pending_refresh: Default::default(),
            force_refresh: false,
            focus,
            root: None,
            roots: Vec::new(),
            explorer: None,
            subscriptions: Vec::new(),
            tabs: Vec::new(),
            active: None,
            tab_scroll: ScrollHandle::new(),
            error: None,
            refresh_notice: None,
            loading: false,
            loading_path: None,
            load_task: None,
            incoming_files: std::collections::VecDeque::new(),
            incoming_file_task: None,
            picker_task: None,
            picking: false,
            save_task: None,
            save_cancel: None,
            saving: None,
            editing_locked: crate::app::workspace_editors_locked(cx),
            usage: None,
            scroll_usage: None,
            scroll_usage_task: None,
            usage_task: None,
            usage_activation: None,
            telemetry_edited: Default::default(),
            autosave: Default::default(),
            save_queue: Default::default(),
            confirm_task: None,
            close_request: None,
            recovery_keys: Default::default(),
            edit_revision: 0,
            quit_approval: None,
            quit_discards: Vec::new(),
            context_menu_task: None,
        };
        workspace
            .subscriptions
            .push(cx.observe_window_bounds(window, |view, window, cx| {
                if !view.file_fullscreen {
                    return;
                }
                if window.is_fullscreen() {
                    view.file_fullscreen_seen_window = true;
                } else if view.file_fullscreen_seen_window {
                    view.set_file_fullscreen(false, window);
                    view.sidebar_window_width = 0.;
                    cx.notify();
                }
            }));

        workspace.usage_activation =
            Some(cx.observe_window_activation(window, |view, window, cx| {
                view.update_usage(window, cx);
                if window.is_window_active() && view.page == Page::Settings {
                    view.integrations.update(cx, |view, cx| view.reload(cx));
                }
                cx.notify();
            }));
        workspace.start_usage_timer(window, cx);
        workspace.reader_toolbar_task = Some(cx.spawn_in(window, async move |workspace, cx| {
            loop {
                let id = super::native_toolbar_bridge::ToolbarEvents::next(&toolbar_events).await;
                // Native actions belong to this window even while the workspace
                // temporarily leaves its render tree (for example, tab switching).
                if cx
                    .update(|window, cx| {
                        workspace.update(cx, |view, cx| {
                            view.run_native_toolbar_action(&id, window, cx);
                        })
                    })
                    .and_then(|result| result)
                    .is_err()
                {
                    break;
                }
            }
        }));
        if let Some(path) = path {
            workspace.open_path(path, window, cx);
        }
        workspace
    }
}

impl Workspace {
    pub(crate) fn is_home(&self) -> bool {
        self.page == Page::Start
    }

    pub(crate) fn show_home(&mut self, cx: &mut Context<Self>) {
        self.page = Page::Start;
        self.sidebar_window_width = 0.;
        if let Some(explorer) = &self.explorer {
            explorer.update(cx, |view, cx| view.clear_clicked_selection(cx));
        }
        self.sync_workspace(cx);
        cx.notify();
    }
}
