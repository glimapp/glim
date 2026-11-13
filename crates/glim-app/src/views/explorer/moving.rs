use super::{Explorer, ExplorerEvent};
use gpui_kit::{
    component::{ActiveTheme, Icon, h_flex},
    *,
};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(super) struct DraggedEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub storage: Option<String>,
}
impl DraggedEntry {
    pub fn accepts(&self, directory: &Path, storage: &Option<String>) -> bool {
        self.storage == *storage
            && directory != self.path
            && self.path.parent() != Some(directory)
            && (!self.is_dir || !directory.starts_with(&self.path))
    }
}
impl Render for DraggedEntry {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_2()
            .px_2()
            .py_1()
            .max_w(px(280.))
            .rounded(px(6.))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(px(12.))
            .child(if self.is_dir {
                Icon::new(gpui_kit::assets::IconName::Folder)
                    .size(px(16.))
                    .into_any_element()
            } else {
                crate::views::file_icons::file_icon(&self.path, cx).into_any_element()
            })
            .child(
                div().truncate().child(
                    self.path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                ),
            )
    }
}

impl Explorer {
    pub(super) fn storage_key(&self) -> Option<String> {
        self.remote
            .as_ref()
            .map(|session| format!("{}:{}", session.connection.id, session.connection.address()))
    }

    pub(super) fn start_move_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.drag_scroll.is_some() || self.move_scroll_step(window, cx).is_none() {
            return;
        }
        self.drag_scroll = Some(cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(32))
                    .await;
                let scrolling = view
                    .update_in(cx, |view, window, cx| {
                        let step = cx
                            .has_active_drag()
                            .then(|| view.move_scroll_step(window, cx))
                            .flatten();
                        let Some(step) = step else {
                            view.drag_scroll = None;
                            return false;
                        };
                        let scroll = &view.scroll.0.borrow().base_handle;
                        let offset = scroll.offset();
                        let y = (offset.y + px(step)).clamp(-scroll.max_offset().y, px(0.));
                        if y == offset.y {
                            view.drag_scroll = None;
                            return false;
                        }
                        view.drag_hover = None;
                        scroll.set_offset(point(offset.x, y));
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !scrolling {
                    return;
                }
            }
        }));
    }

    fn move_scroll_step(&self, window: &Window, cx: &App) -> Option<f32> {
        let bounds = self.scroll.0.borrow().base_handle.bounds();
        let mouse = window.mouse_position();
        if !bounds.contains(&mouse) {
            return None;
        }
        let edge = px(super::row_height(cx));
        if mouse.y < bounds.top() + edge {
            Some(12.)
        } else if mouse.y > bounds.bottom() - edge {
            Some(-12.)
        } else {
            None
        }
    }
    pub(super) fn hover_move_folder(
        &mut self,
        path: &Path,
        root_id: u64,
        event: &DragMoveEvent<DraggedEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let hovered = event.bounds.contains(&event.event.position)
            && event.drag(cx).accepts(path, &self.storage_key())
            && !crate::app::workspace_editors_locked(cx);
        if !hovered {
            if self
                .drag_hover
                .as_ref()
                .is_some_and(|(pending, _)| pending == path)
            {
                self.drag_hover = None;
            }
            return;
        }
        let Some(index) = self.root_ids.iter().position(|id| *id == root_id) else {
            return;
        };
        if self.expanded[index].contains(path)
            || self
                .drag_hover
                .as_ref()
                .is_some_and(|(pending, _)| pending == path)
        {
            return;
        }
        let path = path.to_path_buf();
        let target = path.clone();
        let bounds = event.bounds;
        let task = cx.spawn_in(window, async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(650))
                .await;
            let _ = view.update_in(cx, |view, window, cx| {
                if !cx.has_active_drag() || !bounds.contains(&window.mouse_position()) {
                    return;
                }
                if let Some(index) = view.root_ids.iter().position(|id| *id == root_id) {
                    view.expanded[index].insert(target.clone());
                    view.load_directory(target, cx);
                    view.rebuild();
                    view.expansion_changed(cx);
                    cx.notify();
                }
            });
        });
        self.drag_hover = Some((path, task));
    }

    pub(crate) fn did_move_entry(
        &mut self,
        source: &Path,
        destination: &Path,
        cx: &mut Context<Self>,
    ) {
        if !self.roots.iter().any(|root| {
            source.starts_with(root) || destination.starts_with(root) || root.starts_with(source)
        }) {
            return;
        }
        let relocate =
            |path: &PathBuf| crate::app::file_move::relocated_path(path, source, destination);
        self.drag_hover = None;
        self.drag_scroll = None;
        self.tasks.clear();
        self.expansion_tasks.clear();
        self.pending_refresh.clear();
        self.directory_update = None;
        self.media_tasks.clear();
        self.media_presence.clear();
        self.directories.clear();
        for root in &mut self.roots {
            *root = relocate(root);
        }
        for paths in &mut self.expanded {
            *paths = paths.iter().map(relocate).collect();
        }
        self.clicked_path = self.clicked_path.as_ref().map(relocate);
        self.preserve_next_reveal = None;
        self.pending_reveal = None;
        self.pending_reveal_root = None;
        self.rebuild();
        self.refresh(cx);
        self.reveal_path(destination.to_path_buf(), cx);
        cx.emit(ExplorerEvent::SelectionChanged);
        cx.notify();
    }
}
