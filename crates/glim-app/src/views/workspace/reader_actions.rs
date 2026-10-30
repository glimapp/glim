use crate::views::reader::Reader;
use gpui_kit::assets::IconName;

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
    Back,
    Forward,
    Preview,
    Source,
    WordWrap,
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
            Self::Back => "back",
            Self::Forward => "forward",
            Self::Preview => "reader-preview",
            Self::Source => "reader-source",
            Self::WordWrap => "word-wrap",
            Self::FileFullscreen => "file-fullscreen",
            Self::Update => "update-available",
        }
    }
}

impl ToolbarItem {
    pub(super) fn new(action: ToolbarAction) -> Self {
        let (label, icon) = match action {
            ToolbarAction::Extract => ("Extract…", Some(IconName::Folder)),
            ToolbarAction::CancelExtract => ("Extracting… · Cancel", Some(IconName::X)),
            ToolbarAction::Sidebar => ("Toggle Sidebar", Some(IconName::PanelLeft)),
            ToolbarAction::Back => ("Back", Some(IconName::ArrowLeft)),
            ToolbarAction::Forward => ("Forward", Some(IconName::ArrowRight)),
            ToolbarAction::Preview => ("Preview", Some(IconName::Eye)),
            ToolbarAction::Source => ("Source", Some(IconName::Code)),
            ToolbarAction::WordWrap => ("Word wrap", None),
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
pub(super) fn document_items(reader: &Reader, word_wrap: bool) -> Vec<ToolbarItem> {
    let mut items = Vec::new();
    if reader.is_archive() {
        items.push(ToolbarItem::new(ToolbarAction::Extract));
    }
    if reader.can_preview() {
        let mut preview = ToolbarItem::new(ToolbarAction::Preview);
        if reader.is_json() {
            preview.label = "JSON tree";
            preview.icon = Some(IconName::ListTree);
        } else if reader.is_csv() {
            preview.label = if reader
                .path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("tsv"))
            {
                "TSV table"
            } else {
                "CSV table"
            };
            preview.icon = Some(IconName::Rows2);
        }
        preview.selected = reader.toggle_label() == "Source";
        let mut source = ToolbarItem::new(ToolbarAction::Source);
        source.selected = !preview.selected;
        items.extend([preview, source]);
    }
    if reader.can_word_wrap() {
        let mut wrap = ToolbarItem::new(ToolbarAction::WordWrap);
        wrap.selected = word_wrap;
        wrap.enabled = !reader.can_preview() || reader.toggle_label() != "Source";
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
            self.save_editor(editor, window, cx);
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
        let path = reader.path.clone();
        self.picking = true;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(glim_services::locale::tr("Choose extraction location").into()),
        });
        self.picker_task = Some(cx.spawn_in(window, async move |view, cx| {
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
