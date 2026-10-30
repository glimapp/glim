use crate::app::actions::{NewFile, OpenFile, OpenFolder};
use gpui_kit::{
    assets::IconName,
    component::{
        ActiveTheme, Icon, Sizable,
        button::{Button, ButtonCustomVariant, ButtonVariants},
        h_flex,
        menu::{ContextMenuExt, PopupMenuItem},
        v_flex,
    },
    prelude::FluentBuilder,
    *,
};

fn quick_action(
    id: &'static str,
    label: &'static str,
    shortcut: &'static str,
    icon: IconName,
    share: f32,
    cx: &App,
) -> Button {
    let primary = id == "welcome-new-workspace";
    let ink = if primary {
        super::accent(cx)
    } else {
        cx.theme().muted_foreground
    };
    let surface = if primary {
        cx.theme().background.blend(ink.opacity(0.14))
    } else {
        cx.theme().background
    };
    let border = if primary {
        cx.theme().border.blend(ink.opacity(0.40))
    } else {
        cx.theme().border
    };
    Button::new(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .color(surface)
                .hover(surface.blend(cx.theme().foreground.opacity(0.04)))
                .active(surface.blend(cx.theme().foreground.opacity(0.08))),
        )
        .bg(surface)
        .flex_1()
        .flex_basis(relative(share))
        .min_w_0()
        .h(px(60.))
        .px_3()
        .rounded(px(10.))
        .border_1()
        .border_color(border)
        .accessibility_label(glim_services::locale::tr(label))
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_2()
                .child(
                    Icon::new(icon)
                        .size(px(20.))
                        .text_color(ink)
                        .flex_shrink_0(),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .text_left()
                        .font_weight(if primary {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::MEDIUM
                        })
                        .text_color(if primary { ink } else { cx.theme().foreground })
                        .child(glim_services::locale::tr(label)),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(10.))
                        .text_color(cx.theme().muted_foreground)
                        .child(shortcut),
                ),
        )
}

pub fn welcome(
    current: &super::workspace::Workspace,
    _current_id: EntityId,
    cx: &mut Context<super::workspace::Workspace>,
) -> impl IntoElement {
    let recent = cx
        .try_global::<crate::app::RecentWorkspaces>()
        .map(|history| history.0.clone())
        .unwrap_or_default();
    let more = recent.len() > current.recent_visible();
    let recent_count = recent.len();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let visible = recent
        .into_iter()
        .take(current.recent_visible())
        .collect::<Vec<_>>();
    let content = v_flex()
        .w_full()
        .max_w(px(860.))
        .min_w_0()
        .flex_shrink_0()
        .gap_8()
        .child(
            h_flex()
                .items_center()
                .gap_3()
                .child(
                    img("branding/glim-welcome.png")
                        .size(px(48.))
                        .object_fit(ObjectFit::Contain)
                        .flex_shrink_0(),
                )
                .child(
                    div()
                        .text_size(px(34.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Glim"),
                ),
        )
        .child(
            h_flex()
                .w_full()
                .gap_3()
                .child(
                    quick_action(
                        "welcome-new-workspace",
                        "Open New Workspace",
                        "⌘⇧O",
                        IconName::FolderOpen,
                        0.5,
                        cx,
                    )
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenFolder), cx)),
                )
                .child(
                    quick_action(
                        "welcome-open-file",
                        "Open File…",
                        "⌘O",
                        IconName::FileText,
                        0.25,
                        cx,
                    )
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenFile), cx)),
                )
                .child(
                    quick_action(
                        "welcome-new-file",
                        "New File",
                        "⌘N",
                        IconName::FilePlus,
                        0.25,
                        cx,
                    )
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(NewFile), cx)),
                ),
        )
        .when(!visible.is_empty(), |view| {
            view.child(
                v_flex()
                    .w_full()
                    .min_w_0()
                    .gap_3()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(glim_services::locale::tr("Recent Workspaces")),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(recent_count.to_string()),
                            ),
                    )
                    .child(v_flex().w_full().min_w_0().gap_1().children(
                        visible.into_iter().enumerate().map(|(index, workspace)| {
                            let title = workspace.title();
                            let paths = workspace
                                .folders
                                .iter()
                                .map(|path| {
                                    home.as_ref()
                                        .and_then(|home| path.strip_prefix(home).ok())
                                        .map(|relative| format!("~/{}", relative.display()))
                                        .unwrap_or_else(|| path.display().to_string())
                                })
                                .collect::<Vec<_>>()
                                .join(" · ");
                            let is_current = current.matches_recent_workspace(&workspace);
                            let open = workspace.clone();
                            let new_window = workspace.clone();
                            let remove_id = workspace.id.clone();
                            let context_workspace = workspace.clone();
                            let weak = cx.entity().downgrade();
                            let context_weak = weak;
                            let accent = super::accent(cx);
                            h_flex()
                                .w_full()
                                .min_w_0()
                                .h(px(44.))
                                .items_center()
                                .rounded(px(7.))
                                .when(is_current, |row| row.bg(accent.opacity(0.08)))
                                .hover(|row| row.bg(cx.theme().muted.opacity(0.55)))
                                .child(
                                    Button::new(("recent-workspace", index))
                                        .ghost()
                                        .flex_1()
                                        .min_w_0()
                                        .h_full()
                                        .px_3()
                                        .justify_start()
                                        .accessibility_label(format!("{title}: {paths}"))
                                        .child(
                                            h_flex()
                                                .w_full()
                                                .min_w_0()
                                                .items_center()
                                                .gap_3()
                                                .child(
                                                    Icon::new(IconName::Folder)
                                                        .size(px(17.))
                                                        .text_color(if is_current {
                                                            accent
                                                        } else {
                                                            cx.theme().muted_foreground
                                                        })
                                                        .flex_shrink_0(),
                                                )
                                                .child(
                                                    div()
                                                        .w(relative(0.30))
                                                        .min_w_0()
                                                        .flex_shrink_0()
                                                        .truncate()
                                                        .text_sm()
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .child(title),
                                                )
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .truncate()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(paths),
                                                )
                                                .child(
                                                    div()
                                                        .w(px(40.))
                                                        .flex_shrink_0()
                                                        .text_xs()
                                                        .text_color(accent)
                                                        .when(is_current, |label| {
                                                            label.child(glim_services::locale::tr(
                                                                "Current",
                                                            ))
                                                        }),
                                                ),
                                        )
                                        .on_click(cx.listener(move |view, _, window, cx| {
                                            view.open_recent_here(open.clone(), window, cx)
                                        })),
                                )
                                .child(
                                    Button::new(("recent-new-window", index))
                                        .custom(
                                            ButtonCustomVariant::new(cx)
                                                .foreground(
                                                    cx.theme().muted_foreground.opacity(0.70),
                                                )
                                                .hover(cx.theme().foreground.opacity(0.04))
                                                .active(cx.theme().foreground.opacity(0.08)),
                                        )
                                        .small()
                                        .icon(Icon::new(IconName::ArrowUpRight).size(px(14.)))
                                        .tooltip(glim_services::locale::tr("Open in New Window"))
                                        .accessibility_label(glim_services::locale::tr(
                                            "Open in New Window",
                                        ))
                                        .on_click(move |_, _, cx| {
                                            crate::app::open_recent_workspace(
                                                new_window.clone(),
                                                cx,
                                            )
                                        }),
                                )
                                .child(
                                    Button::new(("recent-remove", index))
                                        .custom(
                                            ButtonCustomVariant::new(cx)
                                                .foreground(
                                                    cx.theme().muted_foreground.opacity(0.70),
                                                )
                                                .hover(cx.theme().foreground.opacity(0.04))
                                                .active(cx.theme().foreground.opacity(0.08)),
                                        )
                                        .small()
                                        .mr_2()
                                        .icon(Icon::new(IconName::X).size(px(14.)))
                                        .tooltip(glim_services::locale::tr(
                                            "Remove from Recent Workspaces",
                                        ))
                                        .accessibility_label(glim_services::locale::tr(
                                            "Remove from Recent Workspaces",
                                        ))
                                        .on_click(move |_, _, cx| {
                                            crate::app::remove_recent_workspace(
                                                remove_id.clone(),
                                                cx,
                                            )
                                        }),
                                )
                                .context_menu(move |menu, _, _| {
                                    let open = context_workspace.clone();
                                    let weak = context_weak.clone();
                                    let new_window = context_workspace.clone();
                                    let remove = context_workspace.id.clone();
                                    menu.item(
                                        PopupMenuItem::new(glim_services::locale::tr(
                                            "Open in Current Window",
                                        ))
                                        .on_click(
                                            move |_, window, cx| {
                                                let _ = weak.update(cx, |view, cx| {
                                                    view.open_recent_here(open.clone(), window, cx)
                                                });
                                            },
                                        ),
                                    )
                                    .item(
                                        PopupMenuItem::new(glim_services::locale::tr(
                                            "Open in New Window",
                                        ))
                                        .on_click(
                                            move |_, _, cx| {
                                                crate::app::open_recent_workspace(
                                                    new_window.clone(),
                                                    cx,
                                                )
                                            },
                                        ),
                                    )
                                    .separator()
                                    .item(
                                        PopupMenuItem::new(glim_services::locale::tr(
                                            "Remove from Recent Workspaces",
                                        ))
                                        .on_click(
                                            move |_, _, cx| {
                                                crate::app::remove_recent_workspace(
                                                    remove.clone(),
                                                    cx,
                                                )
                                            },
                                        ),
                                    )
                                })
                        }),
                    ))
                    .when(more, |view| {
                        view.child(
                            Button::new("recent-more")
                                .ghost()
                                .icon(IconName::ChevronDown)
                                .label(glim_services::locale::tr("Show more"))
                                .on_click(cx.listener(|view, _, _, cx| view.show_more_recent(cx))),
                        )
                    }),
            )
        });
    v_flex()
        .id("welcome-page")
        .size_full()
        .min_h_0()
        .overflow_y_scroll()
        .items_center()
        .px_6()
        .pt(px(24.))
        .pb_8()
        .child(content)
}

#[cfg(test)]
mod tests {
    use gpui_kit::{
        AppContext, Bounds, Pixels, Point, TestAppContext, WindowBounds, WindowOptions, px, size,
        test::TestWindowExt,
    };

    #[gpui_kit::test]
    fn quick_actions_stay_in_one_row_at_minimum_window_width(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        for (width, height) in [(800., 480.), (1100., 760.), (1470., 855.)] {
            let (handle, _) = cx.update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: Point::default(),
                            size: size(px(width), px(height)),
                        })),
                        ..Default::default()
                    },
                    cx,
                    |window, cx| {
                        cx.new(|cx| super::super::workspace::Workspace::new(None, window, cx))
                    },
                )
                .unwrap()
            });
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let mut previous: Option<Bounds<Pixels>> = None;
                for id in [
                    "welcome-new-workspace",
                    "welcome-open-file",
                    "welcome-new-file",
                ] {
                    let bounds = window.find(id).bounds();
                    assert!(bounds.size.width >= px(105.), "{id} too narrow at {width}");
                    assert!(bounds.origin.x >= px(0.));
                    assert!(bounds.origin.x + bounds.size.width <= px(width));
                    if let Some(previous) = previous {
                        assert!(
                            bounds.origin.y >= previous.origin.y + previous.size.height
                                || bounds.origin.x >= previous.origin.x + previous.size.width,
                            "quick actions overlap at {width}"
                        );
                    }
                    previous = Some(bounds);
                }
            })
            .unwrap();
        }
    }
}
