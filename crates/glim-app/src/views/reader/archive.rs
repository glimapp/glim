use glim_services::{archive::Archive, locale::tr};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        ActiveTheme, Disableable, Icon, Sizable,
        button::{Button, ButtonVariants},
        h_flex, v_flex,
    },
    *,
};
use std::collections::BTreeMap;

pub(super) struct ArchiveReader {
    archive: Archive,
    prefix: String,
    rows: Vec<(String, bool, u64)>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    selected: Option<usize>,
    pub(super) extract: Option<(String, super::ArchiveAction)>,
}
impl ArchiveReader {
    pub fn new(archive: Archive, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            focus: cx.focus_handle(),
            selected: None,
            extract: None,
            archive,
            prefix: String::new(),
            rows: Vec::new(),
            scroll: UniformListScrollHandle::new(),
        };
        view.rebuild();
        view
    }
    fn rebuild(&mut self) {
        let mut rows = BTreeMap::new();
        for entry in &self.archive.entries {
            if entry.name.starts_with("__MACOSX/") {
                continue;
            }
            let Some(tail) = entry.name.strip_prefix(&self.prefix) else {
                continue;
            };
            if tail.is_empty() {
                continue;
            }
            let name = tail.split('/').next().unwrap_or_default();
            let directory = tail.contains('/') || entry.directory;
            let row = rows.entry(name.to_owned()).or_insert((directory, 0u64));
            row.1 = row.1.saturating_add(entry.size);
        }
        self.rows = rows
            .into_iter()
            .map(|(name, (dir, size))| (name, dir, size))
            .collect();
        self.rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    }
}
fn bytes(n: u64) -> String {
    if n >= 1_048_576 {
        format!("{:.1} MB", n as f64 / 1_048_576.)
    } else if n >= 1024 {
        format!("{:.1} KB", n as f64 / 1024.)
    } else {
        format!("{n} B")
    }
}
impl ArchiveReader {
    fn navigate(&mut self, prefix: String, cx: &mut Context<Self>) {
        self.prefix = prefix;
        self.selected = None;
        self.rebuild();
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }
    fn up(&mut self, cx: &mut Context<Self>) {
        let trimmed = self.prefix.trim_end_matches('/');
        let prefix = trimmed
            .rsplit_once('/')
            .map_or(String::new(), |(p, _)| format!("{p}/"));
        self.navigate(prefix, cx);
    }
}
impl Focusable for ArchiveReader {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for ArchiveReader {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::assets::IconName;
        let title = self
            .archive
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mut crumbs = h_flex()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .items_center()
            .gap_1()
            .child(
                Button::new("archive-root")
                    .ghost()
                    .small()
                    .label(tr("Archive contents"))
                    .on_click(cx.listener(|v, _, _, cx| v.navigate(String::new(), cx))),
            );
        let mut prefix = String::new();
        for (i, name) in self
            .prefix
            .trim_end_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .enumerate()
        {
            prefix.push_str(name);
            prefix.push('/');
            let path = prefix.clone();
            crumbs = crumbs
                .child(
                    Icon::new(IconName::ChevronRight)
                        .size(px(12.))
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    Button::new(("archive-crumb", i))
                        .ghost()
                        .small()
                        .label(name.to_owned())
                        .on_click(cx.listener(move |v, _, _, cx| v.navigate(path.clone(), cx))),
                );
        }
        v_flex()
            .id("archive-browser")
            .track_focus(&self.focus)
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_key_down(cx.listener(|v, event: &KeyDownEvent, _, cx| {
                match event.keystroke.key.as_str() {
                    "up" | "down" if !v.rows.is_empty() => {
                        let index = if event.keystroke.key == "down" {
                            v.selected.map_or(0, |i| (i + 1).min(v.rows.len() - 1))
                        } else {
                            v.selected.unwrap_or(0).saturating_sub(1)
                        };
                        v.selected = Some(index);
                        v.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
                        cx.notify();
                    }
                    "enter" | "right" => {
                        if let Some((name, true, _)) =
                            v.selected.and_then(|i| v.rows.get(i)).cloned()
                        {
                            v.navigate(format!("{}{name}/", v.prefix), cx);
                        }
                    }
                    "backspace" | "left" if !v.prefix.is_empty() => v.up(cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .size(px(36.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.))
                            .bg(cx.theme().muted)
                            .child(crate::views::file_icons::file_icon(&self.archive.path, cx)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "ZIP · {} {} · {}",
                                        self.archive.entries.len(),
                                        tr("Archive entries"),
                                        bytes(self.archive.bytes)
                                    )),
                            ),
                    )
                    .when_some(self.extract.clone(), |el, (label, action)| {
                        el.child(
                            Button::new("archive-extract-primary")
                                .primary()
                                .small()
                                .label(label)
                                .on_click(move |_, window, cx| action(window, cx)),
                        )
                    }),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .child(
                        Button::new("archive-up")
                            .ghost()
                            .small()
                            .icon(IconName::ArrowUp)
                            .tooltip(tr("Up one folder"))
                            .accessibility_label(tr("Up one folder"))
                            .disabled(self.prefix.is_empty())
                            .on_click(cx.listener(|v, _, _, cx| v.up(cx))),
                    )
                    .child(crumbs),
            )
            .child(
                h_flex()
                    .w_full()
                    .px_4()
                    .h(px(28.))
                    .items_center()
                    .gap_4()
                    .bg(cx.theme().muted.opacity(0.5))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(div().flex_1().min_w_0().pl(px(28.)).child(tr("Name")))
                    .child(
                        div()
                            .w(px(100.))
                            .flex_shrink_0()
                            .text_right()
                            .child(tr("Uncompressed size")),
                    )
                    .child(div().w(px(20.)).flex_shrink_0()),
            )
            .child(
                uniform_list(
                    "archive-entries",
                    self.rows.len(),
                    cx.processor(|v, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|i| {
                                let (name, dir, size) = v.rows[i].clone();
                                h_flex()
                                    .id(i)
                                    .w_full()
                                    .h(px(36.))
                                    .px_4()
                                    .gap_4()
                                    .items_center()
                                    .text_sm()
                                    .when(v.selected == Some(i), |el| el.bg(cx.theme().secondary))
                                    .hover(|s| s.bg(cx.theme().muted))
                                    .child(
                                        h_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .gap_3()
                                            .items_center()
                                            .child(if dir {
                                                Icon::new(IconName::Folder)
                                                    .size(px(16.))
                                                    .text_color(cx.theme().primary)
                                                    .into_any_element()
                                            } else {
                                                crate::views::file_icons::file_icon(
                                                    std::path::Path::new(&name),
                                                    cx,
                                                )
                                                .into_any_element()
                                            })
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .truncate()
                                                    .child(name.clone()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .w(px(100.))
                                            .flex_shrink_0()
                                            .text_right()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(bytes(size)),
                                    )
                                    .child(div().w(px(20.)).flex_shrink_0().child(if dir {
                                        Icon::new(IconName::ChevronRight)
                                            .size(px(12.))
                                            .text_color(cx.theme().muted_foreground)
                                            .into_any_element()
                                    } else {
                                        div().into_any_element()
                                    }))
                                    .on_click(cx.listener(move |v, _, window, cx| {
                                        window.focus(&v.focus, cx);
                                        v.selected = Some(i);
                                        if dir {
                                            v.navigate(format!("{}{name}/", v.prefix), cx);
                                        } else {
                                            cx.notify();
                                        }
                                    }))
                            })
                            .collect()
                    }),
                )
                .w_full()
                .flex_1()
                .min_h_0()
                .track_scroll(&self.scroll),
            )
            .when(self.rows.is_empty(), |el| {
                el.child(
                    div()
                        .w_full()
                        .py_6()
                        .text_center()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr("This folder is empty")),
                )
            })
            .child(
                h_flex()
                    .w_full()
                    .px_4()
                    .py_2()
                    .gap_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(div().flex_1().child(format!(
                        "{} {}",
                        self.rows.len(),
                        tr("Archive entries")
                    )))
                    .child(tr("Extract to a new workspace")),
            )
    }
}
