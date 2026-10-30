use glim_services::{archive::Archive, locale::tr};
use gpui_kit::{
    component::{ActiveTheme, Icon, Sizable, button::Button, h_flex, v_flex},
    *,
};
use std::collections::BTreeMap;

pub(super) struct ArchiveReader {
    archive: Archive,
    prefix: String,
    rows: Vec<(String, bool, u64)>,
    scroll: UniformListScrollHandle,
}
impl ArchiveReader {
    pub fn new(archive: Archive) -> Self {
        let mut view = Self {
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
impl Render for ArchiveReader {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .min_h_0()
            .bg(cx.theme().background)
            .child(
                v_flex()
                    .p_6()
                    .gap_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        h_flex()
                            .gap_3()
                            .child(Icon::new(gpui_kit::assets::IconName::Folder))
                            .child(
                                div().text_lg().font_weight(FontWeight::SEMIBOLD).child(
                                    self.archive
                                        .path
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string(),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "ZIP · {} {} · {}",
                                self.archive.entries.len(),
                                tr("Archive entries"),
                                bytes(self.archive.bytes)
                            )),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr("Browse folders, then extract to a new workspace.")),
                    ),
            )
            .child(
                h_flex()
                    .px_4()
                    .py_2()
                    .gap_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("archive-up")
                            .small()
                            .label(tr("Back"))
                            .on_click(cx.listener(|view, _, _, cx| {
                                let trimmed = view.prefix.trim_end_matches('/');
                                view.prefix = trimmed
                                    .rsplit_once('/')
                                    .map_or(String::new(), |(parent, _)| format!("{parent}/"));
                                view.rebuild();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_sm()
                            .child(if self.prefix.is_empty() {
                                tr("Archive contents")
                            } else {
                                self.prefix.clone()
                            }),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr("Uncompressed size")),
                    ),
            )
            .child(
                uniform_list(
                    "archive-entries",
                    self.rows.len(),
                    cx.processor(|view, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|i| {
                                let (name, dir, size) = view.rows[i].clone();
                                h_flex()
                                    .id(i)
                                    .h(px(38.))
                                    .px_4()
                                    .gap_3()
                                    .items_center()
                                    .border_b_1()
                                    .border_color(cx.theme().border)
                                    .hover(|s| s.bg(cx.theme().muted))
                                    .child(Icon::new(if dir {
                                        gpui_kit::assets::IconName::Folder
                                    } else {
                                        gpui_kit::assets::IconName::File
                                    }))
                                    .child(div().flex_1().truncate().text_sm().child(name.clone()))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(bytes(size)),
                                    )
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        if dir {
                                            view.prefix.push_str(&name);
                                            view.prefix.push('/');
                                            view.rebuild();
                                            cx.notify();
                                        }
                                    }))
                            })
                            .collect()
                    }),
                )
                .flex_1()
                .min_h_0()
                .track_scroll(&self.scroll),
            )
    }
}
