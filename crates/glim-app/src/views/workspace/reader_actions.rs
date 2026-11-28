use super::Page;
use crate::views::reader::Reader;
use gpui_kit::{App, Context, Window, assets::IconName};

impl super::Workspace {
    pub(super) fn show_reader_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::views::native_menu_bridge::MenuItem;
        use glim_services::locale::tr;
        use gpui_kit::ClipboardItem;

        enum Command {
            Copy(String),
            SelectAll(gpui_kit::Entity<gpui_kit::base::TextViewState>),
            Reader(ToolbarAction),
        }
        let Some(reader) = self.active_reader() else {
            return;
        };
        let target = super::header::Target::from_reader(reader);
        let mut items = Vec::new();
        let mut commands = Vec::new();
        if let Some(state) = reader.preview_text_state() {
            // Capture selection before menu tracking can move document focus.
            let selected = state.read(cx).selected_text();
            items.push(MenuItem::new(tr("Copy")).enabled(!selected.is_empty()));
            commands.push(Some(Command::Copy(selected)));
            items.push(MenuItem::new(tr("Select All")));
            commands.push(Some(Command::SelectAll(state)));
            items.push(MenuItem::separator());
            commands.push(None);
        }
        for item in self.reader_actions(cx) {
            items.push(
                MenuItem::new(tr(item.label))
                    .enabled(item.enabled)
                    .checked(item.selected)
                    .symbol(item.native_symbol()),
            );
            commands.push(Some(Command::Reader(item.action)));
        }
        self.show_context_menu(items, window, cx, move |view, index, window, cx| {
            if view
                .active_reader()
                .map(super::header::Target::from_reader)
                .as_ref()
                != Some(&target)
            {
                return;
            }
            match commands.get_mut(index).and_then(Option::take) {
                Some(Command::Copy(text)) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
                Some(Command::SelectAll(state)) => state.update(cx, |state, cx| {
                    state.focus_handle().focus(window, cx);
                    state.select_all(cx);
                }),
                Some(Command::Reader(action)) => view.run_toolbar_action(action, window, cx),
                None => {}
            }
        });
    }
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
    ExportPdf,
    OpenInBrowser,
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
            Self::ExportPdf => "export-pdf",
            Self::OpenInBrowser => "open-in-browser",
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
            ToolbarAction::ExportPdf | ToolbarAction::OpenInBrowser
                if matches!(self.icon, Some(IconName::Hourglass)) =>
            {
                "hourglass"
            }
            ToolbarAction::ExportPdf => "arrow.down.document",
            ToolbarAction::OpenInBrowser => "arrow.up.right.square",
            ToolbarAction::FileFullscreen if self.selected => "arrow.down.right.and.arrow.up.left",
            ToolbarAction::FileFullscreen => "arrow.up.left.and.arrow.down.right",
            ToolbarAction::Extract => "archivebox",
            ToolbarAction::CancelExtract => "xmark",
            ToolbarAction::Sidebar => "sidebar.left",
            ToolbarAction::Update => "arrow.down.circle",
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
            ToolbarAction::ExportPdf => ("Export PDF…", Some(IconName::FileDown)),
            ToolbarAction::OpenInBrowser => ("Open in Browser", Some(IconName::ExternalLink)),
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
    fn export_markdown_pdf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::app::reader_jobs::{self, JobClass, Priority};
        use glim_services::{locale::tr, work::Cancellation};
        use gpui_kit::component::WindowExt;

        if self.picking {
            return;
        }
        if self.pdf_export_task.take().is_some() {
            cx.notify();
            return;
        }
        let Some(reader) = self.active_reader() else {
            return;
        };
        let Some((source, base, format)) = reader.markdown_export_snapshot(cx) else {
            return;
        };
        let path = &reader.document.path;
        let name = path.with_extension("pdf");
        let name = name.file_name().unwrap_or_default().to_string_lossy();
        let directory = if reader.document.remote.is_some() {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| "/".into())
        } else {
            path.parent()
                .unwrap_or_else(|| std::path::Path::new("/"))
                .to_owned()
        };
        let title = reader.title.to_string();
        let selection = cx.prompt_for_new_path(&directory, Some(&name));
        self.picking = true;
        let (cancel, guard) = Cancellation::task();
        self.pdf_export_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let outcome = selection.await;
            let Ok(open) = view.update(cx, |view, cx| {
                view.picking = false;
                if !matches!(outcome, Ok(Ok(Some(_)))) {
                    view.pdf_export_task = None;
                }
                cx.notify();
                view.all_open_document_paths(cx)
            }) else {
                return;
            };
            let Ok(Ok(Some(path))) = outcome else { return };
            let result = reader_jobs::run(
                cx.background_executor().clone(),
                JobClass::Native,
                Priority::Interactive,
                cancel,
                move |cancel| {
                    anyhow::ensure!(
                        path.extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")),
                        "{}",
                        tr("Choose a filename ending in .pdf")
                    );
                    let destination =
                        glim_services::files::prepare_save_destination(&path, &open, cancel)?;
                    let text = source.to_string();
                    let text = format
                        .as_ref()
                        .map(|path| glim_services::text_formats::markdown(path, &text))
                        .unwrap_or(text);
                    glim_services::markdown_export::export_pdf(
                        &text,
                        &base,
                        &title,
                        &destination,
                        cancel,
                    )
                },
            )
            .await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.pdf_export_task = None;
                match result {
                    Ok(0) => window.push_notification(tr("PDF exported"), cx),
                    Ok(_) => window.push_notification(
                        tr("PDF exported with unavailable images or diagrams"),
                        cx,
                    ),
                    Err(error) => {
                        view.error = Some(format!("{}\n{error:#}", tr("PDF export failed")).into());
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn open_markdown_in_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::app::reader_jobs::{self, JobClass, Priority};
        use glim_services::{locale::tr, work::Cancellation};
        use gpui_kit::component::WindowExt;

        if self.markdown_browser_task.is_some() {
            return;
        }
        let Some(reader) = self.active_reader() else {
            return;
        };
        let Some((source, base, format)) = reader.markdown_export_snapshot(cx) else {
            return;
        };
        let title = reader.title.to_string();
        let (cancel, guard) = Cancellation::task();
        self.markdown_browser_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let exported = reader_jobs::run(
                cx.background_executor().clone(),
                JobClass::Native,
                Priority::Interactive,
                cancel,
                move |cancel| {
                    let text = source.to_string();
                    let text = format
                        .as_ref()
                        .map(|path| glim_services::text_formats::markdown(path, &text))
                        .unwrap_or(text);
                    glim_services::markdown_export::export_html(&text, &base, &title, cancel)
                },
            )
            .await;
            let result = match exported {
                Ok((path, unavailable)) => crate::app::markdown_export::open_in_browser(&path)
                    .await
                    .map(|()| unavailable)
                    .map_err(|error| error.context(path.display().to_string())),
                Err(error) => Err(error),
            };
            let _ = view.update_in(cx, |view, window, cx| {
                view.markdown_browser_task = None;
                match result {
                    Ok(0) => {}
                    Ok(_) => window.push_notification(
                        tr("Some images or diagrams could not be included in the HTML preview"),
                        cx,
                    ),
                    Err(error) => {
                        view.error = Some(
                            format!(
                                "{}\n{error:#}",
                                tr("Could not open Markdown in the browser")
                            )
                            .into(),
                        );
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

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
                        glim_core::RecentWorkspace {
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
        if self.page == Page::Document
            && self
                .active_reader()
                .is_some_and(Reader::can_export_markdown)
        {
            let mut export = ToolbarItem::new(ToolbarAction::ExportPdf);
            export.enabled = !self.picking;
            if self.pdf_export_task.is_some() && !self.picking {
                export.label = "Rendering PDF… · Cancel";
                export.icon = Some(IconName::Hourglass);
            }
            document.push(export);
            let mut browser = ToolbarItem::new(ToolbarAction::OpenInBrowser);
            if self.markdown_browser_task.is_some() {
                browser.enabled = false;
                browser.label = "Preparing browser preview…";
                browser.icon = Some(IconName::Hourglass);
            }
            document.push(browser);
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
            ToolbarAction::ExportPdf => self.export_markdown_pdf(window, cx),
            ToolbarAction::OpenInBrowser => self.open_markdown_in_browser(window, cx),
            ToolbarAction::Find => {
                if let Some(reader) = self.active_reader() {
                    reader.toggle_find(window, cx);
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
                    self.search_results.clear();
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

impl super::Workspace {
    fn print_content(&self) -> Option<crate::app::markdown_export::PrintContent> {
        use crate::app::markdown_export::PrintContent;
        let reader = self.active_reader()?;
        if self.page != Page::Document || self.print_task.is_some() || reader.is_unavailable() {
            return None;
        }
        let path = &reader.document.path;
        if reader.can_export_markdown() {
            Some(PrintContent::Pdf)
        } else if (reader.mode() == glim_core::ReaderMode::Preview
            && reader.telemetry_kind() == "html")
            || (reader.telemetry_kind() == "native"
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")))
            || (reader.toolbar_profile() == "paged"
                && reader.mode() == glim_core::ReaderMode::Preview
                && glim_core::formats::FormatSet::Html.matches(path))
        {
            Some(PrintContent::Preview)
        } else if reader.is_full_text_editor() && reader.editor().is_some() {
            Some(PrintContent::Text)
        } else if matches!(reader.telemetry_kind(), "image" | "animated_image")
            && !reader.document.trashed
            && reader.document.remote.is_none()
        {
            Some(PrintContent::Image)
        } else {
            None
        }
    }

    pub(super) fn can_print(&self) -> bool {
        self.print_content().is_some()
    }

    pub(super) fn print_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::app::{
            markdown_export::{PrintContent, show_print_panel},
            reader_jobs::{self, JobClass, Priority},
        };
        use glim_services::{locale::tr, work::Cancellation};
        use std::os::unix::ffi::OsStrExt;

        let Some(content) = self.print_content() else {
            return;
        };
        let Some(reader) = self.active_reader() else {
            return;
        };
        let title = reader.title.to_string();
        let result = match content {
            PrintContent::Pdf => {
                let Some((source, base, format)) = reader.markdown_export_snapshot(cx) else {
                    return;
                };
                let job_title = title.clone();
                let (cancel, guard) = Cancellation::task();
                self.print_task = Some(cx.spawn_in(window, async move |view, cx| {
                    let _guard = guard;
                    let result = reader_jobs::run(
                        cx.background_executor().clone(),
                        JobClass::Native,
                        Priority::Interactive,
                        cancel,
                        move |cancel| {
                            let directory = tempfile::tempdir()?;
                            let path = directory.path().join("document.pdf");
                            let output =
                                glim_services::files::prepare_save_destination(&path, &[], cancel)?;
                            let text = source.to_string();
                            let text = format
                                .as_ref()
                                .map(|path| glim_services::text_formats::markdown(path, &text))
                                .unwrap_or(text);
                            let missing = glim_services::markdown_export::export_pdf(
                                &text, &base, &job_title, &output, cancel,
                            )?;
                            Ok((directory, path, missing))
                        },
                    )
                    .await;
                    let _ = view.update_in(cx, |view, window, cx| {
                        view.print_task = None;
                        let result = result.and_then(|(directory, path, missing)| {
                            if missing > 0 {
                                use gpui_kit::component::WindowExt;
                                window.push_notification(
                                    tr("PDF exported with unavailable images or diagrams"),
                                    cx,
                                );
                            }
                            show_print_panel(
                                window,
                                content,
                                path.as_os_str().as_bytes(),
                                &title,
                                Some(directory),
                            )
                        });
                        if let Err(error) = result {
                            view.error = Some(format!("{error:#}").into());
                        }
                        cx.notify();
                    });
                }));
                cx.notify();
                return;
            }
            PrintContent::Preview => show_print_panel(window, content, &[], &title, None),
            PrintContent::Image => show_print_panel(
                window,
                content,
                reader.document.path.as_os_str().as_bytes(),
                &title,
                None,
            ),
            PrintContent::Text => {
                let Some(editor) = reader.editor() else {
                    return;
                };
                show_print_panel(
                    window,
                    content,
                    editor.read(cx).text().to_string().as_bytes(),
                    &title,
                    None,
                )
            }
        };
        if let Err(error) = result {
            self.error = Some(format!("{error:#}").into());
            cx.notify();
        }
    }
}
