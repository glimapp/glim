use super::reader_actions::{ToolbarAction, ToolbarItem};
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
        mut controls: crate::views::reader::controls::ActionElements,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::views::reader::controls::{action_layout, preset};
        controls.extend(self.reader_actions(cx).into_iter().map(|item| {
            (
                item.action.id(),
                self.toolbar_button(item, cx).into_any_element(),
            )
        }));
        if self.saving.is_some() && self.save_cancel.is_some() {
            controls.push((
                "cancel-file-save",
                crate::views::icon_button(
                    "cancel-file-save",
                    gpui_kit::assets::IconName::X,
                    glim_services::locale::tr("Cancel save"),
                )
                .on_click(cx.listener(|view, _, _, cx| view.cancel_save(cx)))
                .into_any_element(),
            ));
        }
        controls.push((
            "hide-reader-toolbar",
            crate::views::icon_button(
                "hide-reader-toolbar",
                gpui_kit::assets::IconName::ChevronDown,
                glim_services::locale::tr("Hide file toolbar"),
            )
            .ghost()
            .on_click(cx.listener(|view, _, _, cx| {
                view.reader_toolbar_collapsed = true;
                view.reader_toolbar_expanded = false;
                cx.refresh_windows();
                cx.notify();
            }))
            .into_any_element(),
        ));
        let profile = self
            .active_reader()
            .map_or("unavailable", |reader| reader.toolbar_profile());
        let mut candidates: std::collections::HashMap<_, _> = controls.into_iter().collect();
        let sidebar = if !self.file_fullscreen {
            self.navigation_width()
        } else {
            0.
        };
        let available = (f32::from(window.viewport_size().width)
            - sidebar
            - f32::from(crate::views::reader::toolbar_trailing_inset(window, cx))
            - 64.)
            .clamp(100., 620.);
        let mut used = 0.;
        let mut previous_group = "";
        let mut row = h_flex().gap_1().items_center().flex_shrink_0();
        let mut extra = h_flex()
            .gap_1()
            .items_center()
            .flex_wrap()
            .max_w(px(available));
        let mut overflow = false;
        for id in preset(profile) {
            let Some(control) = candidates.remove(id) else {
                continue;
            };
            let (group, width) = action_layout(id);
            let group_gap = if previous_group.is_empty() || previous_group == group {
                0.
            } else {
                9.
            };
            let pinned = matches!(*id, "file-fullscreen" | "hide-reader-toolbar");
            let item = div()
                .w(px(width))
                .h(px(28.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .child(control);
            if !pinned && used + width + group_gap + 4. > available - 96. {
                overflow = true;
                extra = extra.child(item);
            } else {
                if group_gap > 0. {
                    row = row.child(div().w(px(1.)).h(px(16.)).mx(px(4.)).bg(cx.theme().border));
                }
                used += width + group_gap + 4.;
                previous_group = group;
                row = row.child(item);
            }
        }
        if overflow {
            row = row.child(
                crate::views::icon_button(
                    "more-reader-actions",
                    gpui_kit::assets::IconName::ChevronsRight,
                    glim_services::locale::tr("More file actions"),
                )
                .selected(self.reader_toolbar_expanded)
                .on_click(cx.listener(|view, _, _, cx| {
                    view.reader_toolbar_expanded = !view.reader_toolbar_expanded;
                    cx.notify();
                })),
            );
        }
        gpui_kit::component::v_flex()
            .gap_1()
            .items_center()
            .when(overflow && self.reader_toolbar_expanded, |view| {
                view.child(extra)
            })
            .child(row)
            .into_any_element()
    }

    fn toolbar_button(&self, item: ToolbarItem, cx: &mut Context<Self>) -> Button {
        let action = item.action;
        let selected = item.selection() == Some(true);
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
                    .color(if self.toolbar_feedback == Some(action.id()) {
                        cx.theme().foreground.opacity(0.12)
                    } else {
                        cx.theme().transparent
                    })
                    .foreground(if selected {
                        crate::views::accent(cx)
                    } else {
                        cx.theme().foreground
                    })
                    .hover(cx.theme().foreground.opacity(0.10))
                    .active(if selected {
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
            .selected(selected)
            .disabled(!item.enabled)
            .on_click(cx.listener(move |view, _, window, cx| {
                view.run_toolbar_action(action, window, cx);
            }));
        button
    }

    pub(super) fn navigation_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .h(px(28.))
            .items_center()
            .gap_1()
            .flex_shrink_0()
            .child(self.toolbar_button(ToolbarItem::new(ToolbarAction::Sidebar), cx))
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
                "symbol": item.native_symbol(), "enabled": item.enabled, "selection": item.selection(),
                "profile": self.active_reader().map_or("unavailable", |reader| reader.toolbar_profile())})
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
                "symbol": "xmark.circle", "enabled": true, "selection": null}));
        }
        items.push(serde_json::json!({"id": "hide-reader-toolbar", "label": glim_services::locale::tr("Hide file toolbar"),
            "moreLabel": glim_services::locale::tr("More file actions"), "restoreLabel": glim_services::locale::tr("Show file toolbar"),
            "roundTop": false,
            "symbol": "chevron.down", "enabled": true, "selection": null}));
        let bytes = serde_json::to_vec(&items).expect("Reader toolbar actions");
        let toolbar = self.reader_toolbar.clone();
        let collapsed = self.reader_toolbar_collapsed;
        canvas(
            |_, _, _| (),
            move |mut bounds, _, window, cx| {
                // Native siblings are outside GPUI's clip stack. Keep the dock
                // in the visible viewport even when reader content is taller.
                bounds = bounds.intersect(&window.content_mask().bounds);
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
}
