use super::reader_actions::{ToolbarAction, ToolbarItem, document_items};
use super::{Page, Workspace};
use gpui_kit::{
    component::{
        ActiveTheme, Disableable, Icon, Selectable, Sizable,
        button::{Button, ButtonCustomVariant, ButtonVariants},
        h_flex,
    },
    prelude::FluentBuilder,
    *,
};

impl Workspace {
    pub(crate) fn content_toolbar(
        &self,
        controls: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut row = h_flex().gap_2().items_center().flex_shrink_0();
        let mut fullscreen = None;
        for item in self.reader_actions(cx) {
            if matches!(item.action, ToolbarAction::FileFullscreen) {
                fullscreen = Some(item);
            } else {
                row = row.child(self.toolbar_button(item, cx));
            }
        }
        row.child(controls)
            .when(self.saving.is_some() && self.save_cancel.is_some(), |row| {
                row.child(
                    Button::new("cancel-file-save")
                        .small()
                        .label(glim_services::locale::tr("Cancel save"))
                        .on_click(cx.listener(|view, _, _, cx| view.cancel_save(cx))),
                )
            })
            .when_some(fullscreen, |row, item| {
                row.child(self.toolbar_button(item, cx))
            })
            .child(
                crate::views::icon_button(
                    "hide-reader-toolbar",
                    gpui_kit::assets::IconName::ChevronDown,
                    glim_services::locale::tr("Hide file toolbar"),
                )
                .ghost()
                .on_click(cx.listener(|view, _, _, cx| {
                    view.reader_toolbar_collapsed = true;
                    cx.refresh_windows();
                    cx.notify();
                })),
            )
            .into_any_element()
    }

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

    fn toolbar_button(&self, item: ToolbarItem, cx: &mut Context<Self>) -> Button {
        let action = item.action;
        let label = glim_services::locale::tr(item.label);
        let icon = match item.icon {
            Some(icon) => Icon::new(icon),
            None if matches!(action, ToolbarAction::FileFullscreen) => {
                Icon::empty().path(if self.file_fullscreen {
                    "navigation/file-fullscreen-exit.svg"
                } else {
                    "navigation/file-fullscreen.svg"
                })
            }
            None => Icon::empty().path("navigation/wrap.svg"),
        };
        let button = Button::new(action.id())
            .custom(
                ButtonCustomVariant::new(cx)
                    .foreground(if item.selected {
                        crate::views::accent(cx)
                    } else {
                        cx.theme().foreground
                    })
                    .hover(cx.theme().foreground.opacity(0.10))
                    .active(if item.selected {
                        crate::views::accent(cx).opacity(0.20)
                    } else {
                        cx.theme().foreground.opacity(0.18)
                    }),
            )
            .small()
            .w(px(28.))
            .h(px(28.))
            .flex_shrink_0()
            .rounded(px(7.))
            .icon(icon.size(px(16.)))
            .tooltip(label.clone())
            .accessibility_label(label.clone())
            .selected(item.selected || self.toolbar_feedback == Some(action.id()))
            .disabled(!item.enabled)
            .on_click(cx.listener(move |view, _, window, cx| {
                view.run_toolbar_action(action, window, cx);
            }));
        if matches!(
            action,
            ToolbarAction::Extract | ToolbarAction::CancelExtract
        ) {
            button.w_auto().label(label)
        } else {
            button
        }
    }

    pub(super) fn navigation_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut back = ToolbarItem::new(ToolbarAction::Back);
        back.enabled = self.history_cursor.is_some_and(|i| i > 0);
        let mut forward = ToolbarItem::new(ToolbarAction::Forward);
        forward.enabled = self
            .history_cursor
            .is_some_and(|i| i + 1 < self.history.len());
        h_flex()
            .h(px(28.))
            .items_center()
            .gap_1()
            .flex_shrink_0()
            .child(self.toolbar_button(ToolbarItem::new(ToolbarAction::Sidebar), cx))
            .when(
                self.sidebar_window_width * self.sidebar_ratio >= 280. || !self.sidebar_visible,
                |row| {
                    row.child(self.toolbar_button(back, cx))
                        .child(self.toolbar_button(forward, cx))
                },
            )
            .child(self.search_button(cx))
            .when(
                self.reader_toolbar_collapsed && self.page == Page::Document,
                |row| row.child(self.restore_reader_toolbar(cx)),
            )
            .child(self.workspace_toolbar(cx))
            .into_any_element()
    }

    pub(super) fn restore_reader_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        Button::new("show-reader-toolbar")
            .small()
            .w(px(28.))
            .h(px(28.))
            .icon(
                Icon::empty()
                    .path("navigation/reader-toolbar.svg")
                    .size(px(18.)),
            )
            .tooltip(glim_services::locale::tr("Show file toolbar"))
            .accessibility_label(glim_services::locale::tr("Show file toolbar"))
            .ghost()
            .on_click(cx.listener(|view, _, _, cx| {
                view.reader_toolbar_collapsed = false;
                cx.refresh_windows();
                cx.notify();
            }))
            .into_any_element()
    }

    pub(super) fn floating_reader_toolbar(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        use gpui_kit::component::WindowExt;
        let mut items: Vec<_> = self.reader_actions(cx).into_iter().map(|item| {
            serde_json::json!({"id": item.action.id(), "label": glim_services::locale::tr(item.label),
                "symbol": item.native_symbol(), "enabled": item.enabled, "selected": item.selected})
        }).collect();
        let visible = !items.is_empty()
            && !self.search_visible
            && !window.has_active_dialog(cx)
            && !window.has_active_sheet(cx)
            && !window
                .context_stack()
                .iter()
                .any(|context| context.contains("PopupMenu") || context.contains("Dialog"));
        if self.saving.is_some() && self.save_cancel.is_some() {
            items.push(serde_json::json!({"id": "cancel-file-save", "label": glim_services::locale::tr("Cancel save"),
                "symbol": "xmark.circle", "enabled": true, "selected": false}));
        }
        items.push(serde_json::json!({"id": "hide-reader-toolbar", "label": glim_services::locale::tr("Hide file toolbar"),
            "moreLabel": glim_services::locale::tr("More file actions"), "restoreLabel": glim_services::locale::tr("Show file toolbar"),
            "roundTop": !self.sidebar_visible,
            "symbol": "chevron.down", "enabled": true, "selected": false}));
        let bytes = serde_json::to_vec(&items).expect("Reader toolbar actions");
        let toolbar = self.reader_toolbar.clone();
        let collapsed = self.reader_toolbar_collapsed;
        canvas(
            |_, _, _| (),
            move |mut bounds, _, window, cx| {
                bounds.size.width = (bounds.size.width
                    - crate::views::reader::toolbar_trailing_inset(window, cx))
                .max(px(0.));
                toolbar.update(bounds, &bytes, visible, collapsed, window);
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
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

    pub(super) fn workspace_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let has_update = crate::app::updates::state(cx)
            .is_some_and(|state| state.read(cx).snapshot.has_update());
        let mut toolbar = h_flex().gap_1().flex_shrink_0();
        if has_update {
            toolbar =
                toolbar.child(self.toolbar_button(ToolbarItem::new(ToolbarAction::Update), cx));
        }
        toolbar.into_any_element()
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
            ToolbarAction::Sidebar => {
                self.sidebar_visible = !self.sidebar_visible;
                self.sidebar_window_width = 0.;
                cx.notify();
            }
            ToolbarAction::Back => self.navigate(false, window, cx),
            ToolbarAction::Forward => self.navigate(true, window, cx),
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
