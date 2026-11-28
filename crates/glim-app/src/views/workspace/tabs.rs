use super::Workspace;
use super::changes::DiffTab;
use super::header::Target;
use crate::views::reader::Reader;
use gpui_kit::{
    component::{
        ActiveTheme, Icon, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
    },
    prelude::FluentBuilder,
    *,
};
use std::{cell::Cell, rc::Rc};

const TAB_HEIGHT: f32 = 30.;

fn tab_frame(
    id: (&'static str, usize),
    label: &str,
    description: &str,
    active: bool,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    cx: &App,
) -> Stateful<Div> {
    let accent = crate::views::accent(cx);
    let background = cx.theme().background;
    h_flex()
        .id(id)
        .role(Role::Tab)
        .aria_label(label.to_owned())
        .aria_description(description.to_owned())
        .aria_selected(active)
        .tab_index(0)
        .focus_visible(move |style| style.border_color(accent))
        .relative()
        .child(
            canvas(move |area, _, _| bounds.set(area), |_, _, _, _| {})
                .absolute()
                .inset_0(),
        )
        .h(px(TAB_HEIGHT))
        .border_1()
        .border_color(if active {
            accent.opacity(0.4)
        } else {
            gpui_kit::transparent_black()
        })
        .font_weight(if active {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        })
        .flex_shrink_0()
        .gap_1()
        .pl(px(7.))
        .pr(px(3.))
        .rounded(px(7.))
        .bg(if active {
            background.blend(accent.opacity(0.09))
        } else {
            background
        })
        .cursor_pointer()
}

fn tab_label(
    id: (&'static str, usize),
    label: String,
    description: String,
    icon: impl IntoElement,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    cx: &Context<Workspace>,
) -> Stateful<Div> {
    let title = label.clone();
    h_flex()
        .id(id)
        .h_full()
        .gap(px(5.))
        .on_hover(cx.listener(move |view, hovered: &bool, window, _| {
            if *hovered {
                view.path_hint
                    .show(&title, &description, false, bounds.get(), window);
            } else {
                view.path_hint.hide_for(&description, bounds.get());
            }
        }))
        .child(icon)
        .child(div().max_w(px(210.)).truncate().text_xs().child(label))
}

fn tab_close_button(id: (&'static str, usize), label: &str, dirty: bool, cx: &App) -> Button {
    let title = glim_services::locale::tr(if dirty {
        "Unsaved changes · Close"
    } else {
        "Close"
    });
    Button::new(id)
        .ghost()
        .xsmall()
        .w(px(22.))
        .h(px(22.))
        .when(dirty, |button| {
            button.child(div().size(px(9.)).rounded_full().bg(cx.theme().blue))
        })
        .when(!dirty, |button| {
            button.icon(Icon::new(gpui_kit::assets::IconName::X).size(px(12.)))
        })
        .text_color(cx.theme().muted_foreground)
        .tooltip(title.clone())
        .accessibility_label(format!("{title} {label}"))
}

#[derive(Clone)]
struct DraggedTab {
    workspace: EntityId,
    source: WeakEntity<Workspace>,
    window: AnyWindowHandle,
    target: super::header::Target,
    label: String,
}
impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let source = self.source.clone();
        let source_window = self.window;
        let target = self.target.clone();
        h_flex()
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let source = source.clone();
                        let target = target.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture
                                || event.button != MouseButton::Left
                                || window.window_handle() != source_window
                                || !cx.has_active_drag()
                                || Bounds::new(Point::default(), window.viewport_size())
                                    .contains(&event.position)
                            {
                                return;
                            }
                            cx.stop_active_drag(window);
                            let _ =
                                source.update(cx, |view, cx| view.detach_tab(&target, window, cx));
                            cx.stop_propagation();
                        });
                    },
                )
                .absolute()
                .size_0(),
            )
            .px_3()
            .py_1()
            .rounded(px(7.))
            .bg(cx.theme().secondary)
            .text_xs()
            .child(self.label.clone())
    }
}

#[derive(PartialEq, Eq)]
pub(super) enum TabIdentity {
    Pending(std::path::PathBuf),
    Document(u64),
    Diff(super::changes::DiffTab, Option<String>),
}

#[derive(PartialEq, Eq)]
enum TabTarget {
    Document(Target),
    Diff(DiffTab),
}

#[derive(Default)]
pub(super) struct TabCloseTargets {
    pub(super) documents: Vec<Target>,
    diffs: Vec<DiffTab>,
}

impl TabCloseTargets {
    pub(super) fn document(target: Target) -> Self {
        Self {
            documents: vec![target],
            ..Self::default()
        }
    }

    fn is_empty(&self) -> bool {
        self.documents.is_empty() && self.diffs.is_empty()
    }
}

enum TabMenuAction {
    Rename(u64),
    Close(TabCloseTargets),
    Trash(std::path::PathBuf),
    Detach(super::header::Target),
    CopyPath(std::path::PathBuf),
    FileHistory(std::path::PathBuf),
    Reveal(std::path::PathBuf),
}

impl Workspace {
    fn begin_tab_rename(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(reader) = self.tabs.iter().find(|reader| {
            reader.document.id() == id
                && reader.document.restore_on_launch()
                && matches!(
                    reader.session_tab(cx),
                    glim_core::session::SessionTab::File { .. }
                )
        }) else {
            return;
        };
        if !self.file_actions_available(cx) {
            return;
        }
        let path = reader.document.path.clone();
        self.path_hint.hide();
        self.tab_rename = None;
        self.tab_rename = Some((
            id,
            crate::views::inline_rename::InlineRename::new(
                path,
                false,
                window,
                cx,
                Self::finish_tab_rename,
            ),
        ));
        cx.notify();
    }

    fn finish_tab_rename(&mut self, submit: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, rename)) = &self.tab_rename else {
            return;
        };
        let operation = if submit
            && self.tabs.iter().any(|reader| {
                reader.document.id() == *id && reader.document.owns_path(&rename.path)
            }) {
            match rename.destination(cx) {
                Ok(destination) => {
                    destination.map(|destination| (rename.path.clone(), destination))
                }
                Err(message) => {
                    self.report_recovery_error(message, cx);
                    return;
                }
            }
        } else {
            None
        };
        self.tab_rename = None;
        if submit {
            window.focus(&self.focus, cx);
        }
        if let Some((source, destination)) = operation {
            self.rename_entry(source, destination, window, cx);
        }
        cx.notify();
    }

    fn can_transfer_tabs(&self, cx: &App) -> bool {
        !self.is_saving()
            && !self.loading
            && self.confirm_task.is_none()
            && !crate::app::updates::busy(cx)
    }

    fn detach_tab(
        &mut self,
        target: &super::header::Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_transfer_tabs(cx) {
            self.error =
                Some("A file operation is in progress. Please retry after it finishes.".into());
            cx.notify();
            return;
        }
        let Some(index) = self
            .tabs
            .iter()
            .position(|reader| super::header::Target::from_reader(reader) == *target)
        else {
            return;
        };
        if !self.tabs[index].can_move_to_window(cx) {
            self.error =
                Some("A file operation is in progress. Please retry after it finishes.".into());
            cx.notify();
            return;
        }
        // Create the destination before taking ownership of the live reader.
        let (destination, handle) = match crate::app::open_detached_window(cx) {
            Ok(destination) => destination,
            Err(error) => {
                self.error = Some(format!("{error:#}").into());
                cx.notify();
                return;
            }
        };
        let reader = self.take_tab(index, window, cx);
        // The just-created window cannot close between these synchronous updates.
        handle
            .update(cx, |_, window, cx| {
                destination.update(cx, |view, cx| view.receive_detached_tab(reader, window, cx));
            })
            .expect("Newly created tab window");
    }

    pub(crate) fn take_file_tab(
        &mut self,
        path: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Reader> {
        if self.incoming_file_task.is_some() || !self.can_transfer_tabs(cx) {
            return None;
        }
        let index = self
            .tabs
            .iter()
            .position(|reader| reader.document.owns_path(path))?;
        self.tabs[index]
            .can_move_to_window(cx)
            .then(|| self.take_tab(index, window, cx))
    }

    fn take_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> Reader {
        let reader = self.tabs.remove(index);
        reader.deactivate(cx);
        if let Some(editor) = reader.editor() {
            let id = editor.entity_id();
            self.autosave.remove(&id);
            // The destination takes over the same recovery record.
            self.recovery_keys.remove(&id);
            self.save_queue.retain(|queued| *queued != id);
        }
        self.active = selection_after_close(self.active, index, self.tabs.len() + 1);
        self.observe_autosave(window, cx);
        if let Some(active) = self.active {
            self.activate_tab(active, window, cx);
            self.tabs[active].activate(cx);
        }
        if let Some(session) = self.window_session(cx) {
            crate::app::workspace_sessions::remember(session, cx);
        }
        self.sync_workspace(cx);
        cx.notify();
        reader
    }

    fn receive_detached_tab(
        &mut self,
        reader: Reader,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(remote) = reader.document.remote.clone() {
            self.remote = Some(remote);
            self.recent_id = format!("ssh-workspace-detached-{}", self.recent_id);
            self.roots = reader
                .document
                .path
                .parent()
                .map(|path| vec![path.to_path_buf()])
                .unwrap_or_default();
            self.root = self.roots.first().cloned();
            self.remember_remote(cx);
        }
        self.workspace_name = Some(
            reader
                .document
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| reader.title.to_string()),
        );
        self.receive_system_tab(reader, window, cx);
    }

    pub(crate) fn receive_system_tab(
        &mut self,
        reader: Reader,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_workspace(self.recent_id.clone(), window, cx);
        reader.bind_window(window, cx);
        let editor = reader.editor();
        let autosave = reader.document.autosave_enabled() && reader.is_dirty();
        let index = self.tabs.len();
        self.tabs.push(reader);
        self.observe_autosave(window, cx);
        self.activate_tab(index, window, cx);
        self.tabs[index].activate(cx);
        self.watch_file_parent(window, cx);
        if let Some(editor) = editor {
            self.checkpoint_editor(editor.entity_id(), cx);
            if autosave {
                self.save_editor(editor.entity_id(), window, cx);
            }
        }
        self.sync_workspace(cx);
        self.remember_session(cx);
    }

    pub(super) fn active_reader(&self) -> Option<&Reader> {
        self.active.and_then(|index| self.tabs.get(index))
    }

    pub(super) fn reveal_active_file(&self, cx: &mut Context<Self>) {
        if let (Some(explorer), Some(reader)) = (&self.explorer, self.active_reader())
            && reader.document.restore_on_launch()
        {
            explorer.update(cx, |explorer, cx| {
                explorer.reveal_path(reader.document.path.clone(), cx)
            });
        }
    }

    pub(super) fn activate_tab(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.get(index).is_some() {
            glim_services::telemetry::file_event(
                "tab_activated",
                &self.tabs[index].document.path,
                [],
            );
            log::info!(
                "tab activate file={}",
                self.tabs[index].document.path.display()
            );
            self.page = super::Page::Document;
            self.load_task = None;
            self.loading = false;
            self.load_incoming_file(window, cx);
            self.active = Some(index);
            let document = &self.tabs[index].document;
            if document.remote.is_none()
                && document.restore_on_launch()
                && document
                    .path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
            {
                self.pending_refresh.insert(document.path.clone());
                self.start_pending_refresh(window, cx);
            }
            self.reveal_active_file(cx);
            if !self.explorer_has_focus(window, cx) {
                window.focus(&self.focus, cx);
                if self.tabs[index].editor().is_some()
                    && self.tabs[index].mode() == glim_core::ReaderMode::Source
                {
                    self.tabs[index].focus_source(window, cx);
                }
            }
            self.revealed_tab = None;
            window.set_window_title("Glim");
            self.record_history(cx);
            cx.notify();
        }
    }

    fn close_tabs(
        &mut self,
        targets: TabCloseTargets,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if targets.is_empty() {
            return;
        }
        if !targets.documents.is_empty()
            && (self.is_saving()
                || self.tabs.iter().any(|reader| {
                    reader.is_dirty() && targets.documents.contains(&Target::from_reader(reader))
                }))
        {
            self.confirm_discard(
                super::editing::DiscardAction::CloseTabs(targets),
                window,
                cx,
            );
        } else {
            self.close_tabs_unchecked(targets, window, cx);
        }
    }
    pub(super) fn close_tabs_unchecked(
        &mut self,
        targets: TabCloseTargets,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !targets.documents.is_empty() && self.is_saving() {
            self.error =
                Some("A file operation is in progress. Please retry after it finishes.".into());
            cx.notify();
            return;
        }
        if !targets.diffs.is_empty() {
            self.changes
                .update(cx, |git, cx| git.close_tabs(&targets.diffs, cx));
            self.update_page_after_diff_close(cx);
        }
        if targets.documents.is_empty() {
            return;
        }
        let active = self.active_reader().map(Target::from_reader);
        let previous = self.active.unwrap_or(0);
        self.tabs.retain(|r| {
            let keep = !targets.documents.contains(&Target::from_reader(r));
            if !keep {
                glim_services::telemetry::file_event(
                    "tab_closed",
                    &r.document.path,
                    [("unsaved", glim_services::telemetry::flag(r.is_dirty()))],
                );
                r.deactivate(cx);
            }
            keep
        });
        self.load_task = None;
        self.loading = false;
        self.load_incoming_file(window, cx);
        self.active = active
            .and_then(|target| {
                self.tabs
                    .iter()
                    .position(|r| Target::from_reader(r) == target)
            })
            .or_else(|| (!self.tabs.is_empty()).then(|| previous.min(self.tabs.len() - 1)));
        self.observe_autosave(window, cx);
        if self.page == super::Page::Document {
            let git = self.changes.read(cx);
            let other_diff = (!git.tabs.is_empty()).then(|| git.active_tab().unwrap_or(0));
            if let Some(index) = self.active {
                self.activate_tab(index, window, cx);
            } else if let Some(index) = other_diff.filter(|_| !self.loading) {
                self.activate_diff_tab(index, cx);
            } else {
                window.set_window_title("Glim");
                window.focus(&self.focus, cx);
            }
        }
        self.remember_session(cx);
        cx.notify();
    }
    fn show_tab_menu(&mut self, target: &TabTarget, window: &mut Window, cx: &mut Context<Self>) {
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .map(|reader| TabTarget::Document(Target::from_reader(reader)))
            .chain(
                self.changes
                    .read(cx)
                    .tabs
                    .iter()
                    .cloned()
                    .map(TabTarget::Diff),
            )
            .collect();
        let Some(index) = tabs.iter().position(|tab| tab == target) else {
            return;
        };
        let mut items = Vec::new();
        let mut actions = Vec::new();
        let document = self.tabs.get(index);
        if let (TabTarget::Document(target), Some(reader)) = (target, document) {
            items.push((
                Some(glim_services::locale::tr("Move Tab to New Window")),
                self.can_transfer_tabs(cx) && reader.can_move_to_window(cx),
            ));
            actions.push(Some(TabMenuAction::Detach(target.clone())));
            if reader.document.restore_on_launch()
                && matches!(
                    reader.session_tab(cx),
                    glim_core::session::SessionTab::File { .. }
                )
            {
                items.push((
                    Some(glim_services::locale::tr("Rename…")),
                    self.file_actions_available(cx),
                ));
                actions.push(Some(TabMenuAction::Rename(reader.document.id())));
            }
            if reader.document.draft_directory.is_none() {
                let path = reader.document.path.clone();
                if reader.document.remote.is_none() {
                    items.push((Some(glim_services::locale::tr("Reveal in Finder")), true));
                    actions.push(Some(TabMenuAction::Reveal(path.clone())));
                }
                items.push((Some(glim_services::locale::tr("Copy Absolute Path")), true));
                let relative = self
                    .roots
                    .iter()
                    .filter_map(|root| path.strip_prefix(root).ok())
                    .min_by_key(|relative| relative.components().count())
                    .map(ToOwned::to_owned);
                actions.push(Some(TabMenuAction::CopyPath(path)));
                if let Some(relative) = relative {
                    items.push((Some(glim_services::locale::tr("Copy Relative Path")), true));
                    actions.push(Some(TabMenuAction::CopyPath(relative)));
                }
            }
            if reader.document.draft_directory.is_none()
                && self
                    .git_repositories
                    .iter()
                    .any(|root| reader.document.path.starts_with(root))
            {
                items.push((Some(glim_services::locale::tr("File history")), true));
                actions.push(Some(TabMenuAction::FileHistory(
                    reader.document.path.clone(),
                )));
            }
            items.push((None, true));
            actions.push(None);
        }
        let close_commands: [(&str, fn(usize, usize) -> bool); 4] = [
            ("Close", |tab, clicked| tab == clicked),
            ("Close Tabs to the Right", |tab, clicked| tab > clicked),
            ("Close Other Tabs", |tab, clicked| tab != clicked),
            ("Close All Tabs", |_, _| true),
        ];
        for (label, includes_tab) in close_commands {
            let mut targets = TabCloseTargets::default();
            for (_, tab) in tabs
                .iter()
                .enumerate()
                .filter(|(i, _)| includes_tab(*i, index))
            {
                match tab {
                    TabTarget::Document(target) => targets.documents.push(target.clone()),
                    TabTarget::Diff(target) => targets.diffs.push(target.clone()),
                }
            }
            items.push((Some(glim_services::locale::tr(label)), !targets.is_empty()));
            actions.push(Some(TabMenuAction::Close(targets)));
        }
        if let Some(reader) = document
            && reader.document.draft_directory.is_none()
            && reader.document.remote.is_none()
        {
            items.push((None, true));
            actions.push(None);
            items.push((Some(glim_services::locale::tr("Move to Trash…")), true));
            actions.push(Some(TabMenuAction::Trash(reader.document.path.clone())));
        }
        self.context_menu_task = None;
        let Some(menu) = crate::views::native_menu_bridge::NativeMenu::new(&items, window) else {
            return;
        };
        self.context_menu_task = Some(cx.spawn_in(window, async move |view, cx| {
            let selected = menu.choose(cx).await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.context_menu_task = None;
                if let Some(action) = selected
                    .and_then(|index| actions.get_mut(index))
                    .and_then(Option::take)
                {
                    match action {
                        TabMenuAction::Rename(id) => view.begin_tab_rename(id, window, cx),
                        TabMenuAction::Detach(target) => view.detach_tab(&target, window, cx),
                        TabMenuAction::CopyPath(path) => cx.write_to_clipboard(
                            ClipboardItem::new_string(path.to_string_lossy().into_owned()),
                        ),
                        TabMenuAction::Reveal(path) => cx.reveal_path(&path),
                        TabMenuAction::FileHistory(path) => view.open_git_file_history(&path, cx),
                        TabMenuAction::Close(targets) => view.close_tabs(targets, window, cx),
                        TabMenuAction::Trash(path) => view.confirm_trash(path, window, cx),
                    }
                }
            });
        }));
    }

    fn on_tab_mouse_down(
        &mut self,
        target: &TabTarget,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Right {
            window.prevent_default();
            cx.stop_propagation();
            self.path_hint.hide();
            self.show_tab_menu(target, window, cx);
        }
    }

    pub(super) fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.get(index).is_some_and(|r| r.is_dirty()) || self.is_saving() {
            self.confirm_discard(super::editing::DiscardAction::CloseTab(index), window, cx);
        } else {
            self.close_tab_unchecked(index, window, cx);
        }
    }

    pub(super) fn close_tab_unchecked(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index >= self.tabs.len() {
            return;
        }
        self.load_task = None;
        self.loading = false;
        self.load_incoming_file(window, cx);
        self.active = selection_after_close(self.active, index, self.tabs.len());
        glim_services::telemetry::file_event(
            "tab_closed",
            &self.tabs[index].document.path,
            [(
                "unsaved",
                glim_services::telemetry::flag(self.tabs[index].is_dirty()),
            )],
        );
        self.tabs.remove(index);
        self.observe_autosave(window, cx);
        if let Some(active) = self.active {
            self.activate_tab(active, window, cx);
        } else {
            window.set_window_title("Glim");
            window.focus(&self.focus, cx);
        }
        self.remember_session(cx);
        cx.notify();
    }
}

impl Workspace {
    fn reorder_tab(
        &mut self,
        source: &super::header::Target,
        destination: &super::header::Target,
        after: bool,
        cx: &mut Context<Self>,
    ) {
        // Resolve identities at drop time: a tab may have closed during the drag.
        let Some(from) = self
            .tabs
            .iter()
            .position(|tab| super::header::Target::from_reader(tab) == *source)
        else {
            return;
        };
        let Some(target) = self
            .tabs
            .iter()
            .position(|tab| super::header::Target::from_reader(tab) == *destination)
        else {
            return;
        };
        let gap = target + usize::from(after);
        let to = if from < gap { gap - 1 } else { gap };
        if from == to {
            return;
        }
        let reader = self.tabs.remove(from);
        self.tabs.insert(to, reader);
        self.active = self.active.map(|active| {
            if active == from {
                to
            } else if from < active && active <= to {
                active - 1
            } else if to <= active && active < from {
                active + 1
            } else {
                active
            }
        });
        self.tab_scroll
            .scroll_to_item(to + usize::from(self.pending_tab_path().is_some()));
        self.remember_session(cx);
        cx.notify();
    }

    fn activate_diff_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        self.changes_expanded = true;
        self.page = super::Page::Changes;
        self.changes
            .update(cx, |git, cx| git.activate_tab(index, cx));
        self.revealed_tab = None;
        cx.notify();
    }
    pub(super) fn cycle_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let git = self.changes.read(cx);
        let n = self.tabs.len() + git.tabs.len();
        if n == 0 {
            return;
        }
        let current = if self.page == super::Page::Changes {
            self.tabs.len() + git.active_tab().unwrap_or(0)
        } else {
            self.active.unwrap_or(0)
        };
        let index = if forward {
            (current + 1) % n
        } else {
            (current + n - 1) % n
        };
        if index < self.tabs.len() {
            self.activate_tab(index, window, cx);
        } else {
            self.activate_diff_tab(index - self.tabs.len(), cx);
        }
    }
    pub(super) fn close_diff_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(tab) = self.changes.read(cx).tabs.get(index).cloned() else {
            return;
        };
        self.changes
            .update(cx, |git, cx| git.close_tabs(&[tab], cx));
        self.update_page_after_diff_close(cx);
    }

    fn update_page_after_diff_close(&mut self, cx: &mut Context<Self>) {
        if self.changes.read(cx).tabs.is_empty() && self.page == super::Page::Changes {
            self.page = super::Page::Document;
        }
        cx.notify();
    }

    pub(super) fn tab_bar(
        &mut self,
        leading_inset: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tabs = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, reader)| {
                let active = self.page == super::Page::Document && self.active == Some(index);
                let id = reader.document.id();
                let target = super::header::Target::from_reader(reader);
                let label = if reader.document.draft_directory.is_some() {
                    reader.title.to_string()
                } else {
                    reader
                        .document
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                };
                let hint_path = if reader.document.trashed {
                    glim_services::locale::tr("Deleted · Kept until you quit · ⌘S to save a copy")
                } else if reader.document.draft_directory.is_some() {
                    glim_services::locale::tr("Not saved to disk")
                } else if let Some(remote) = &reader.document.remote {
                    format!(
                        "{} · {}\n{}",
                        glim_services::locale::tr(
                            if reader.can_edit_remote_text() && !reader.document.trashed {
                                "Remote"
                            } else {
                                "Remote · Read-only"
                            }
                        ),
                        remote.connection.label(),
                        reader.document.path.display()
                    )
                } else {
                    reader.document.path.display().to_string()
                };
                let hint_bounds = Rc::new(Cell::new(Bounds::default()));
                let drop_bounds = hint_bounds.clone();
                let drop_target = target.clone();
                let dragged = DraggedTab {
                    workspace: cx.entity_id(),
                    source: cx.entity().downgrade(),
                    window: window.window_handle(),
                    target: target.clone(),
                    label: label.clone(),
                };
                let workspace_id = cx.entity_id();
                let drop_color = cx.theme().secondary;

                let menu_target = TabTarget::Document(target);
                let frame = tab_frame(
                    ("document-tab", index),
                    &label,
                    &hint_path,
                    active,
                    hint_bounds.clone(),
                    cx,
                );
                let close =
                    tab_close_button(("close-tab", index), &reader.title, reader.is_dirty(), cx)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            cx.stop_propagation();
                            view.finish_tab_rename(false, window, cx);
                            view.close_tab(index, window, cx);
                        }));
                if let Some((_, rename)) = self.tab_rename.as_ref().filter(|(target, rename)| {
                    *target == id && reader.document.owns_path(&rename.path)
                }) {
                    let text_style = window.text_style();
                    let label_width = window
                        .text_system()
                        .layout_line(&label, px(12.), &[text_style.to_run(label.len())], None)
                        .width;
                    return frame
                        .child(
                            h_flex()
                                .gap(px(5.))
                                .child(crate::views::file_icons::file_icon(
                                    &reader.document.path,
                                    cx,
                                ))
                                .child(
                                    rename
                                        .field(px(24.), cx)
                                        .w((label_width + px(6.)).clamp(px(76.), px(210.))),
                                ),
                        )
                        .child(close)
                        .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" {
                                view.finish_tab_rename(false, window, cx);
                                window.focus(&view.focus, cx);
                                cx.stop_propagation();
                            }
                        }));
                }
                let can_rename = reader.document.restore_on_launch()
                    && matches!(
                        reader.session_tab(cx),
                        glim_core::session::SessionTab::File { .. }
                    );
                frame
                    .on_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                        if can_rename && event.keystroke.key == "enter" {
                            view.begin_tab_rename(id, window, cx);
                            cx.stop_propagation();
                        } else if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            view.activate_tab(index, window, cx);
                            view.tabs[index].activate(cx);
                            cx.stop_propagation();
                        }
                    }))
                    .on_drag(dragged, |drag, _, _, cx| {
                        cx.stop_propagation();
                        cx.new(|_| drag.clone())
                    })
                    .drag_over::<DraggedTab>(move |style, drag, _, _| {
                        if drag.workspace == workspace_id {
                            style.bg(drop_color)
                        } else {
                            style
                        }
                    })
                    .on_drop(cx.listener(move |view, drag: &DraggedTab, window, cx| {
                        if drag.workspace != cx.entity_id() {
                            return;
                        }
                        let bounds = drop_bounds.get();
                        let after =
                            window.mouse_position().x >= bounds.origin.x + bounds.size.width / 2.;
                        view.reorder_tab(&drag.target, &drop_target, after, cx);
                        cx.stop_propagation();
                    }))
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        view.activate_tab(index, window, cx);
                        view.tabs[index].activate(cx);
                    }))
                    .child(
                        tab_label(
                            ("document-tab-label", index),
                            label,
                            hint_path,
                            if self.pending_open_path() == Some(&reader.document.path) {
                                crate::views::loading_icon(
                                    ("file-loading", index),
                                    px(16.),
                                    glim_services::locale::tr("Loading…"),
                                    cx,
                                )
                            } else {
                                crate::views::file_icons::file_icon(&reader.document.path, cx)
                                    .into_any_element()
                            },
                            hint_bounds,
                            cx,
                        )
                        .when(can_rename, |label| {
                            label.on_click(cx.listener(
                                move |view, event: &ClickEvent, window, cx| {
                                    if event.click_count() == 2 && view.file_actions_available(cx) {
                                        cx.stop_propagation();
                                        view.begin_tab_rename(id, window, cx);
                                    }
                                },
                            ))
                        })
                        .when(reader.document.trashed, |label| {
                            label.text_color(cx.theme().muted_foreground)
                        }),
                    )
                    .child(close)
                    .capture_any_mouse_down(cx.listener(
                        move |view, event: &MouseDownEvent, window, cx| {
                            view.on_tab_mouse_down(&menu_target, event, window, cx);
                        },
                    ))
            })
            .collect::<Vec<_>>();
        let pending_tab = self.pending_open_tab(cx);
        let git = self.changes.read(cx);
        let selected = git.active_tab();
        let diff_tabs = git
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let range = git.release_title(tab);
                let label = range.cloned().unwrap_or_else(|| tab.label());
                let description = match (range, tab) {
                    (Some(range), super::changes::DiffTab::Comparison(root, _)) => {
                        format!("{} · {range}", root.display())
                    }
                    _ => tab.description(),
                };
                let active = self.page == super::Page::Changes && selected == Some(index);
                let target = TabTarget::Diff(tab.clone());
                let hint_bounds = Rc::new(Cell::new(Bounds::default()));
                let close = tab_close_button(("close-diff-tab", index), &label, false, cx)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        cx.stop_propagation();
                        view.close_diff_tab(index, cx);
                    }));
                tab_frame(
                    ("diff-tab-row", index),
                    &label,
                    &description,
                    active,
                    hint_bounds.clone(),
                    cx,
                )
                .on_key_down(cx.listener(move |view, event: &KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        view.activate_diff_tab(index, cx);
                        cx.stop_propagation();
                    }
                }))
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.activate_diff_tab(index, cx);
                }))
                .child(tab_label(
                    ("diff-tab-label", index),
                    label,
                    description,
                    Icon::new(gpui_kit::assets::IconName::FileDiff).size(px(16.)),
                    hint_bounds,
                    cx,
                ))
                .child(close)
                .capture_any_mouse_down(cx.listener(
                    move |view, event: &MouseDownEvent, window, cx| {
                        view.on_tab_mouse_down(&target, event, window, cx);
                    },
                ))
            })
            .collect::<Vec<_>>();
        let first_document = usize::from(pending_tab.is_some());
        let target = match self.page {
            super::Page::Document if pending_tab.is_some() => self
                .pending_open_path()
                .map(|path| (TabIdentity::Pending(path.clone()), 0)),
            super::Page::Document => {
                self.active
                    .zip(self.active_reader())
                    .map(|(index, reader)| {
                        (
                            TabIdentity::Document(reader.document.id()),
                            first_document + index,
                        )
                    })
            }
            super::Page::Changes => selected.map(|index| {
                (
                    TabIdentity::Diff(
                        git.tabs[index].clone(),
                        git.release_title(&git.tabs[index]).cloned(),
                    ),
                    first_document + tabs.len() + index,
                )
            }),
            _ => None,
        };
        if self.revealed_tab != target {
            if let Some((_, index)) = &target {
                self.tab_scroll.scroll_to_item(*index);
            }
            self.revealed_tab = target;
        }
        h_flex()
            .h(px(super::TITLE_BAR_HEIGHT))
            .w_full()
            .flex_shrink_0()
            .px_1()
            .pl(px(4. + leading_inset))
            .bg(cx.theme().background)
            .rounded_tl(px(11.))
            .rounded_tr(px(11.))
            .border_b_1()
            .border_color(cx.theme().border)
            .when(
                !tabs.is_empty() || !diff_tabs.is_empty() || pending_tab.is_some(),
                |bar| {
                    bar.child(
                        h_flex()
                            .id("document-tabs")
                            .role(Role::TabList)
                            .gap_1()
                            .track_scroll(&self.tab_scroll)
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_x_scroll()
                            .children(pending_tab)
                            .children(tabs)
                            .children(diff_tabs),
                    )
                },
            )
            .into_any_element()
    }

    fn pending_tab_path(&self) -> Option<&std::path::PathBuf> {
        self.pending_open_path().filter(|path| {
            !self
                .tabs
                .iter()
                .any(|reader| reader.document.owns_path(path))
        })
    }

    fn pending_open_tab(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let path = self.pending_tab_path()?;
        let label = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let status = format!(
            "{} · {}",
            glim_services::locale::tr("Loading…"),
            path.display()
        );
        Some(
            h_flex()
                .id("pending-document-tab")
                .role(Role::Tab)
                .aria_label(label.clone())
                .aria_description(status.clone())
                .aria_selected(false)
                .h(px(TAB_HEIGHT))
                .flex_shrink_0()
                .gap_1()
                .pl(px(7.))
                .pr(px(3.))
                .border_1()
                .border_color(cx.theme().border)
                .rounded(px(7.))
                .bg(cx.theme().background)
                .child(
                    h_flex()
                        .gap(px(5.))
                        .child(crate::views::loading_icon(
                            "pending-file-loading",
                            px(16.),
                            status,
                            cx,
                        ))
                        .child(div().max_w(px(210.)).truncate().text_xs().child(label)),
                )
                .child(
                    Button::new("cancel-pending-file")
                        .ghost()
                        .xsmall()
                        .w(px(22.))
                        .h(px(22.))
                        .icon(
                            gpui_kit::component::Icon::new(gpui_kit::assets::IconName::X)
                                .size(px(12.)),
                        )
                        .text_color(cx.theme().muted_foreground)
                        .tooltip(glim_services::locale::tr("Cancel"))
                        .accessibility_label(glim_services::locale::tr("Cancel"))
                        .on_click(cx.listener(|view, _, window, cx| {
                            cx.stop_propagation();
                            view.cancel_pending_open(window, cx);
                        })),
                )
                .into_any_element(),
        )
    }
}

fn selection_after_close(active: Option<usize>, closed: usize, count: usize) -> Option<usize> {
    active.and_then(|active| {
        if count <= 1 {
            None
        } else if closed < active {
            Some(active - 1)
        } else {
            Some(active.min(count - 2))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::selection_after_close;

    #[test]
    fn closing_another_tab_preserves_the_selected_document() {
        for count in 2..10 {
            let tabs = (0..count).collect::<Vec<_>>();
            for active in 0..count {
                for closed in 0..count {
                    if active == closed {
                        continue;
                    }
                    let next = selection_after_close(Some(active), closed, count).unwrap();
                    let mut remaining = tabs.clone();
                    remaining.remove(closed);
                    assert_eq!(remaining[next], tabs[active]);
                }
            }
        }
    }

    #[test]
    fn closing_active_selects_a_neighbor_and_last_close_clears_selection() {
        assert_eq!(selection_after_close(Some(1), 1, 3), Some(1));
        assert_eq!(selection_after_close(Some(2), 2, 3), Some(1));
        assert_eq!(selection_after_close(Some(0), 0, 1), None);
    }
}
