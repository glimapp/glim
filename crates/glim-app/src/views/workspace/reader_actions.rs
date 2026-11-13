use super::Page;
use crate::views::reader::Reader;
use gpui_kit::{App, Context, Window, assets::IconName};

pub(super) fn preview_text_menu(
    menu: gpui_kit::component::menu::PopupMenu,
    state: gpui_kit::Entity<gpui_kit::base::TextViewState>,
    cx: &gpui_kit::App,
) -> gpui_kit::component::menu::PopupMenu {
    use gpui_kit::{ClipboardItem, component::menu::PopupMenuItem};

    // Capture before the popup takes focus away from the document.
    let selected = state.read(cx).selected_text();
    menu.item(
        PopupMenuItem::new(glim_services::locale::tr("Copy"))
            .disabled(selected.is_empty())
            .on_click(move |_, _, cx| {
                if !selected.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(selected.clone()));
                }
            }),
    )
    .item(
        PopupMenuItem::new(glim_services::locale::tr("Select All")).on_click(
            move |_, window, cx| {
                state.update(cx, |state, cx| {
                    state.focus_handle().focus(window, cx);
                    state.select_all(cx);
                });
            },
        ),
    )
    .separator()
}

#[derive(Clone, Copy)]
pub(super) enum ToolbarAction {
    Extract,
    CancelExtract,
    Sidebar,
    Preview,
    Source,
    WordWrap,
    Find,
    FileFullscreen,
    Update,
}

pub(super) struct ToolbarItem {
    pub(super) action: ToolbarAction,
    pub(super) label: &'static str,
    pub(super) icon: Option<IconName>,
    pub(super) selected: bool,
    pub(super) enabled: bool,
}

impl ToolbarAction {
    pub(super) fn id(self) -> &'static str {
        match self {
            Self::Extract => "archive-extract",
            Self::CancelExtract => "archive-cancel",
            Self::Sidebar => "toggle-sidebar",
            Self::Preview => "reader-preview",
            Self::Source => "reader-source",
            Self::WordWrap => "word-wrap",
            Self::Find => "find-in-file",
            Self::FileFullscreen => "file-fullscreen",
            Self::Update => "update-available",
        }
    }
}

impl ToolbarItem {
    /// Commands have no selection. Modes and switches mirror their owner state.
    pub(super) fn selection(&self) -> Option<bool> {
        match self.action {
            ToolbarAction::Preview
            | ToolbarAction::Source
            | ToolbarAction::WordWrap
            | ToolbarAction::FileFullscreen => Some(self.selected),
            _ => None,
        }
    }

    pub(super) fn native_symbol(&self) -> &'static str {
        match self.action {
            ToolbarAction::Preview if matches!(self.icon, Some(IconName::ListTree)) => {
                "list.bullet.indent"
            }
            ToolbarAction::Preview if matches!(self.icon, Some(IconName::Rows2)) => "tablecells",
            ToolbarAction::Preview => "eye",
            ToolbarAction::Source => "chevron.left.forwardslash.chevron.right",
            ToolbarAction::WordWrap => "arrow.turn.down.left",
            ToolbarAction::Find => "magnifyingglass",
            ToolbarAction::FileFullscreen if self.selected => "arrow.down.right.and.arrow.up.left",
            ToolbarAction::FileFullscreen => "arrow.up.left.and.arrow.down.right",
            ToolbarAction::Extract => "archivebox",
            ToolbarAction::CancelExtract => "xmark",
            _ => "ellipsis",
        }
    }

    pub(super) fn new(action: ToolbarAction) -> Self {
        let (label, icon) = match action {
            ToolbarAction::Extract => ("Extract…", Some(IconName::Folder)),
            ToolbarAction::CancelExtract => ("Extracting… · Cancel", Some(IconName::X)),
            ToolbarAction::Sidebar => ("Toggle Sidebar", Some(IconName::PanelLeft)),
            ToolbarAction::Preview => ("Preview", Some(IconName::Eye)),
            ToolbarAction::Source => ("Source", Some(IconName::Code)),
            ToolbarAction::WordWrap => ("Word wrap", None),
            ToolbarAction::Find => ("Find in file · ⌘F", Some(IconName::Search)),
            ToolbarAction::FileFullscreen => ("Enter File Fullscreen", None),
            ToolbarAction::Update => ("Update Available", Some(IconName::Download)),
        };
        Self {
            action,
            label,
            icon,
            selected: false,
            enabled: true,
        }
    }
}

/// Fixed order: viewing mode, then text layout.
/// Capabilities come from the active reader, not a separate extension allowlist.
pub(super) fn document_items(
    reader: &Reader,
    word_wrap: bool,
    cx: &gpui_kit::App,
) -> Vec<ToolbarItem> {
    let mut items = Vec::new();
    if reader.can_find(cx) {
        items.push(ToolbarItem::new(ToolbarAction::Find));
    }
    if reader.is_archive() {
        items.push(ToolbarItem::new(ToolbarAction::Extract));
    }
    if reader.can_preview() {
        let mut preview = ToolbarItem::new(ToolbarAction::Preview);
        if reader.is_openapi() {
            preview.label = "API reference";
        } else if reader.is_json() {
            preview.label = "JSON tree";
            preview.icon = Some(IconName::ListTree);
        } else if reader.is_csv() {
            preview.label = if reader.table_format()
                == Some(glim_services::documents::presentation::TableFormat::Tsv)
            {
                "TSV table"
            } else {
                "CSV table"
            };
            preview.icon = Some(IconName::Rows2);
        }
        preview.selected = reader.mode() == glim_core::ReaderMode::Preview;
        let mut source = ToolbarItem::new(ToolbarAction::Source);
        source.selected = !preview.selected;
        items.extend([preview, source]);
    }
    if reader.can_word_wrap() {
        let mut wrap = ToolbarItem::new(ToolbarAction::WordWrap);
        wrap.selected = word_wrap;
        wrap.enabled = !reader.can_preview() || reader.mode() != glim_core::ReaderMode::Preview;
        if !wrap.enabled {
            wrap.label = "Word wrap · Source view";
        }
        items.push(wrap);
    }
    items
}

impl super::Workspace {
    pub(super) fn toggle_markdown_task(
        &mut self,
        editor: gpui_kit::EntityId,
        range: std::ops::Range<usize>,
        checked: bool,
        revision: usize,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if self.editing_locked || crate::app::workspace_editors_locked(cx) {
            return;
        }
        let changed = self
            .tabs
            .iter_mut()
            .find(|reader| {
                reader
                    .editor()
                    .is_some_and(|state| state.entity_id() == editor)
            })
            .is_some_and(|reader| reader.toggle_task(range, checked, revision, window, cx));
        if changed {
            glim_services::telemetry::track(
                "markdown_task_toggled",
                [("completed", glim_services::telemetry::flag(checked))],
            );
            if self.tabs.iter().any(|reader| {
                reader.document.autosave_enabled()
                    && reader
                        .editor()
                        .is_some_and(|state| state.entity_id() == editor)
            }) {
                self.save_editor(editor, window, cx);
            }
        }
        cx.notify();
    }
}

impl super::Workspace {
    pub(super) fn extract_archive(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        use gpui_kit::*;
        if self.picking {
            return;
        }
        let Some(reader) = self.active_reader().filter(|r| r.is_archive()) else {
            return;
        };
        let path = if let Some(remote) = &reader.document.remote {
            match remote.cache_path(&reader.document.path) {
                Ok(path) => path,
                Err(_) => return,
            }
        } else {
            reader.document.path.clone()
        };
        let remote_lease = reader.document.remote.clone();
        self.picking = true;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(glim_services::locale::tr("Choose extraction location").into()),
        });
        self.picker_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _remote_lease = remote_lease;
            let outcome = selection.await;
            let parent = match outcome {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let Some(parent) = parent else {
                let _ = view.update(cx, |view, cx| {
                    view.picking = false;
                    cx.notify();
                });
                return;
            };
            let (cancel, guard) = glim_services::work::Cancellation::task();
            let _guard = guard;
            let result = cx
                .background_executor()
                .spawn(async move { glim_services::archive::extract(&path, &parent, &cancel) })
                .await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.picking = false;
                match result {
                    Ok(path) => view.open_recent_here(
                        glim_services::preferences::RecentWorkspace {
                            id: format!(
                                "extracted-{}",
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_nanos()
                            ),
                            folders: vec![path],
                            name: None,
                        },
                        window,
                        cx,
                    ),
                    Err(error) => {
                        view.error = Some(
                            format!(
                                "{}: {error:#}",
                                glim_services::locale::tr("Extraction failed")
                            )
                            .into(),
                        )
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

impl super::Workspace {
    pub(super) fn set_file_fullscreen(&mut self, enabled: bool, window: &Window) {
        if enabled == self.file_fullscreen {
            return;
        }
        let mut props = self.usage_context();
        props.push(("enabled", glim_services::telemetry::flag(enabled)));
        glim_services::telemetry::track("reader_fullscreen_changed", props);
        if enabled {
            self.file_fullscreen_seen_window = window.is_fullscreen();
            self.file_fullscreen_entered_window = !window.is_fullscreen();
            if self.file_fullscreen_entered_window {
                window.toggle_fullscreen();
            }
        } else {
            if self.file_fullscreen_entered_window && window.is_fullscreen() {
                window.toggle_fullscreen();
            }
            self.file_fullscreen_entered_window = false;
            self.file_fullscreen_seen_window = false;
        }
        self.file_fullscreen = enabled;
    }

    pub(super) fn reader_actions(&self, cx: &App) -> Vec<ToolbarItem> {
        let mut document = if self.page == Page::Document {
            self.active_reader()
                .map(|r| document_items(r, self.word_wrap, cx))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if self.picking && self.active_reader().is_some_and(|r| r.is_archive()) {
            for item in &mut document {
                if matches!(item.action, ToolbarAction::Extract) {
                    *item = ToolbarItem::new(ToolbarAction::CancelExtract);
                }
            }
        }
        if self.page == Page::Document && self.active_reader().is_some() {
            let mut fullscreen = ToolbarItem::new(ToolbarAction::FileFullscreen);
            fullscreen.selected = self.file_fullscreen;
            if self.file_fullscreen {
                fullscreen.label = "Exit File Fullscreen";
            }
            document.push(fullscreen);
        }
        document
    }

    pub(super) fn run_native_toolbar_action(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_visible_workspace(cx) || self.page != Page::Document {
            return;
        }
        match id {
            "hide-reader-toolbar" | "show-reader-toolbar" => {
                self.reader_toolbar_collapsed = id == "hide-reader-toolbar";
                self.record_workspace_action(
                    "reader_toolbar_visibility",
                    [(
                        "collapsed",
                        glim_services::telemetry::flag(self.reader_toolbar_collapsed),
                    )],
                );
                cx.refresh_windows();
                cx.notify();
            }
            "cancel-file-save" => self.cancel_save(cx),
            _ => {
                if let Some(item) = self
                    .reader_actions(cx)
                    .into_iter()
                    .find(|item| item.action.id() == id && item.enabled)
                {
                    self.run_toolbar_action(item.action, window, cx);
                }
            }
        }
    }

    pub(super) fn run_toolbar_action(
        &mut self,
        action: ToolbarAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toolbar_feedback = Some(action.id());
        self.toolbar_feedback_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(280))
                .await;
            let _ = view.update(cx, |view, cx| {
                view.toolbar_feedback = None;
                view.toolbar_feedback_task = None;
                cx.notify();
            });
        }));
        let mut props = self.usage_context();
        props.push(("action", glim_services::telemetry::text(action.id())));
        props.push(("semantic", glim_services::telemetry::text("invocation")));
        glim_services::telemetry::track("reader_action", props);
        match action {
            ToolbarAction::Extract => self.extract_archive(window, cx),
            ToolbarAction::Find => {
                if let Some(reader) = self.active_reader() {
                    reader.find(window, cx);
                }
            }
            ToolbarAction::CancelExtract => {
                self.picker_task = None;
                self.picking = false;
                cx.notify();
            }
            ToolbarAction::Sidebar => self.toggle_sidebar(cx),
            ToolbarAction::Preview | ToolbarAction::Source => {
                if self.active_reader().is_some_and(|r| r.can_preview()) {
                    self.set_reader_preview(matches!(action, ToolbarAction::Preview), window, cx);
                }
            }
            ToolbarAction::WordWrap => {
                if self.active_reader().is_some_and(|r| r.can_word_wrap()) {
                    self.toggle_word_wrap(window, cx);
                }
            }
            ToolbarAction::FileFullscreen => {
                if self.page == Page::Document && self.active_reader().is_some() {
                    self.set_file_fullscreen(!self.file_fullscreen, window);
                    self.results.clear();
                    self.search_task = None;
                    self.history_swipe = None;
                    // Keep the existing reader and its editing/preview state alive.
                    window.focus(&self.focus, cx);
                    cx.notify();
                }
            }
            ToolbarAction::Update => crate::app::updates::show(window, cx),
        }
        cx.notify();
    }
}
