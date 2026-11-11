//! Retained book presentation; preparation and persistence are supplied by its host.
use crate::views::reader::contract::{ReaderHost, ReaderMetric};
use glim_core::{
    books::{Bookmark, Chapter, Format, ReadingState, TocEntry},
    resources::ResourceLease,
};
use gpui_kit::{
    base::{InteractiveElementExt as _, TextView, TextViewState, TextViewStyle},
    component::{
        ActiveTheme, Disableable, Selectable, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        scroll::Scrollbar,
        v_flex,
    },
    prelude::FluentBuilder,
    *,
};
use std::{
    collections::HashMap,
    ops::Range,
    sync::{Arc, Mutex},
};

gpui_kit::actions!(
    glim,
    [
        BookNextPage,
        BookPreviousPage,
        BookNextChapter,
        BookPreviousChapter,
        BookContents
    ]
);

/// Borrowed catalog access avoids copying the book's potentially large directory.
pub(super) trait Catalog: Send + Sync {
    fn title(&self) -> &str;
    fn author(&self) -> &str;
    fn format(&self) -> Format;
    fn chapters(&self) -> &[Chapter];
    fn toc(&self) -> &[TocEntry];
}
#[derive(Clone, Copy)]
pub(super) struct Policy {
    pub label_chars: usize,
    pub toc_depth: usize,
}
pub(super) type ImageResolver = Arc<dyn Fn(&str) -> ImageSource + Send + Sync>;
pub(super) struct PreparedChapter {
    pub html: String,
    pub anchors: HashMap<String, Range<usize>>,
    pub images: ImageResolver,
    pub comic: Option<ImageSource>,
    pub comic_ratio: f32,
    pub page: Option<AnyView>,
    pub memory: ResourceLease,
}
#[derive(Clone)]
pub(super) enum Request {
    Prepare(usize),
    Persist,
    Follow(String),
    ShowText { title: &'static str, text: String },
}
pub(super) struct BookView {
    host: ReaderHost,
    policy: Policy,
    book: Arc<dyn Catalog>,
    _book_memory: ResourceLease,
    pub pending: Arc<Mutex<Option<ReadingState>>>,
    pub state: ReadingState,
    pub revision: u64,
    pub active: bool,
    text: Option<Entity<TextViewState>>,
    paged: Option<AnyView>,
    images: Option<ImageResolver>,
    comic: Option<ImageSource>,
    comic_ratio: f32,
    anchors: HashMap<String, Range<usize>>,
    chapter_memory: Option<ResourceLease>,
    restore: Option<f32>,
    pub anchor: Option<String>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    toc_scroll: UniformListScrollHandle,
    contents: bool,
    bookmarks: bool,
    pub loading: bool,
    pub error: Option<String>,
    _subscription: Option<Subscription>,
}
impl EventEmitter<Request> for BookView {}
impl BookView {
    pub fn new(
        book: Arc<dyn Catalog>,
        host: ReaderHost,
        policy: Policy,
        book_memory: ResourceLease,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            host,
            policy,
            book,
            _book_memory: book_memory,
            pending: Arc::new(Mutex::new(None)),
            state: ReadingState::default(),
            revision: 0,
            active: true,
            text: None,
            paged: None,
            images: None,
            comic: None,
            comic_ratio: 1.,
            anchors: HashMap::new(),
            chapter_memory: None,
            restore: None,
            anchor: None,
            focus,
            scroll: ScrollHandle::new(),
            toc_scroll: UniformListScrollHandle::new(),
            contents: true,
            bookmarks: false,
            loading: true,
            error: None,
            _subscription: None,
        }
    }
    fn excerpt(&self, text: &str) -> String {
        let mut label: String = text.chars().take(self.policy.label_chars).collect();
        if label.len() < text.len() {
            label.push('…');
        }
        label
    }
    pub fn install(&mut self, content: PreparedChapter, cx: &mut Context<Self>) {
        self.paged = content.page;
        self.images = Some(content.images);
        self.comic = content.comic;
        self.comic_ratio = content.comic_ratio;
        self.anchors = content.anchors;
        self.chapter_memory = Some(content.memory);
        let text = cx.new(|cx| TextViewState::html(&content.html, cx));
        self._subscription = Some(cx.observe(&text, |_, _, cx| cx.notify()));
        self.text = Some(text);
        self.scroll = ScrollHandle::new();
        self.restore = Some(self.state.fraction);
        self.loading = false;
        self.error = None;
        cx.notify();
    }
    pub fn go(
        &mut self,
        index: usize,
        fraction: f32,
        anchor: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if index >= self.book.chapters().len() {
            return;
        }
        self.revision = self.revision.wrapping_add(1);
        self.state.chapter = index;
        self.state.fraction = fraction.clamp(0., 1.);
        self.anchor = anchor;
        self.loading = true;
        self.error = None;
        self.paged = None;
        self.release_layout();
        self.save(cx);
        cx.emit(Request::Prepare(index));
        cx.notify();
    }
    pub fn release_layout(&mut self) {
        self._subscription = None;
        self.text = None;
        self.images = None;
        self.comic = None;
        self.anchors = HashMap::new();
        self.chapter_memory = None;
    }
    /// Selection pins the text entity; source fallback remains owned by the adapter.
    pub fn suspend(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.active {
            return self.text.is_some();
        }
        self.position(cx);
        self.active = false;
        let selected = self
            .text
            .as_ref()
            .is_some_and(|text| !text.read(cx).selected_text().is_empty());
        if !selected {
            self.release_layout();
        }
        cx.notify();
        selected
    }
    pub fn has_content(&self) -> bool {
        self.text.is_some() || self.paged.is_some()
    }
    pub fn follow(&mut self, href: &str, cx: &mut Context<Self>) {
        cx.emit(Request::Follow(href.to_owned()));
    }
    fn save(&self, cx: &mut Context<Self>) {
        *self.pending.lock().unwrap() = Some(self.state.clone());
        cx.emit(Request::Persist);
    }
    fn position(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.restore.is_some() || !self.active {
            return;
        }
        let maximum = f32::from(self.scroll.max_offset().y);
        let fraction = if maximum > 0. {
            (-f32::from(self.scroll.offset().y) / maximum).clamp(0., 1.)
        } else {
            0.
        };
        if (self.state.fraction - fraction).abs() > 0.001 {
            self.state.fraction = fraction;
            self.save(cx);
            cx.notify();
        }
    }
    fn page(&mut self, next: bool, cx: &mut Context<Self>) {
        (self.host.action)("ebook", "page", &[ReaderMetric::Flag("forward", next)]);
        if self.loading {
            return;
        }
        if self.book.format() == Format::Comic {
            self.chapter(next, cx);
            return;
        }
        let offset = self.scroll.offset();
        let maximum = self.scroll.max_offset().y;
        let height = self.scroll.bounds().size.height;
        if next && -offset.y >= maximum - px(2.) {
            self.chapter(true, cx);
        } else if !next && offset.y >= -px(2.) {
            self.chapter(false, cx);
        } else {
            self.scroll.set_offset(point(
                px(0.),
                (offset.y + height * if next { -0.9 } else { 0.9 }).clamp(-maximum, px(0.)),
            ));
            self.position(cx);
            cx.notify();
        }
    }
    fn chapter(&mut self, next: bool, cx: &mut Context<Self>) {
        (self.host.action)("ebook", "chapter", &[ReaderMetric::Flag("forward", next)]);
        let index = if next {
            self.state.chapter + 1
        } else {
            self.state.chapter.saturating_sub(1)
        };
        if index != self.state.chapter {
            self.go(index, 0., None, cx);
        }
    }
    fn bookmark(&mut self, cx: &mut Context<Self>) {
        (self.host.action)("ebook", "bookmark", &[]);
        if let Some(index) = self.state.bookmarks.iter().position(|b| {
            b.chapter == self.state.chapter && (b.fraction - self.state.fraction).abs() < 0.02
        }) {
            self.state.bookmarks.remove(index);
        } else {
            self.state.bookmarks.push(Bookmark {
                chapter: self.state.chapter,
                fraction: self.state.fraction,
                label: self.book.chapters()[self.state.chapter]
                    .title
                    .chars()
                    .take(self.policy.label_chars)
                    .collect(),
            });
        }
        self.save(cx);
        cx.notify();
    }
}
impl Render for BookView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tr = self.host.translate;
        let font = self.state.font_size;
        let paper = if self.state.paper {
            cx.theme()
                .background
                .blend(
                    cx.theme()
                        .yellow
                        .opacity(if cx.theme().is_dark() { 0.06 } else { 0.12 }),
                )
        } else {
            cx.theme().background
        };
        let marked = self.state.bookmarks.iter().any(|b| {
            b.chapter == self.state.chapter && (b.fraction - self.state.fraction).abs() < 0.02
        });
        let mut content = div().into_any_element();
        if self.loading {
            content = div()
                .p_8()
                .text_color(cx.theme().muted_foreground)
                .child(tr("Loading chapter…"))
                .into_any_element();
        } else if let Some(error) = &self.error {
            content = div()
                .p_8()
                .whitespace_normal()
                .child(error.clone())
                .into_any_element();
        } else if let Some(image) = &self.comic {
            content = img(image.clone())
                .w_full()
                .h(px((self.state.page_width - 64.) / self.comic_ratio))
                .aspect_ratio(self.comic_ratio)
                .object_fit(ObjectFit::Contain)
                .into_any_element();
        } else if let Some(text) = &self.text {
            let scroll_reveal = self.scroll.clone();
            let images = self
                .images
                .clone()
                .expect("installed chapter image resolver");
            let view = cx.entity().downgrade();
            content = TextView::new(text)
                .scrollable(false)
                .selectable(true)
                .w_full()
                .font_family(if self.state.serif {
                    "Georgia".into()
                } else {
                    cx.theme().font_family.clone()
                })
                .text_size(px(font))
                .line_height(relative(self.state.line_height))
                .style(
                    TextViewStyle::default()
                        .with_foreground(cx.theme().foreground)
                        .with_muted_foreground(cx.theme().muted_foreground)
                        .with_link(cx.theme().primary)
                        .with_selection(cx.theme().selection)
                        .with_border(cx.theme().border)
                        .with_code_background(cx.theme().muted)
                        .with_paragraph_gap(rems(1.2)),
                )
                .image_source(move |uri| images(uri.as_ref()))
                .on_reveal(move |line, _, _| {
                    let bounds = scroll_reveal.bounds();
                    let mut offset = scroll_reveal.offset();
                    if line.bottom() > bounds.bottom() {
                        offset.y -= line.bottom() - bounds.bottom();
                    } else if line.top() < bounds.top() {
                        offset.y += bounds.top() - line.top();
                    }
                    scroll_reveal.set_offset(offset);
                })
                .on_link_click(move |href, _, _, cx| {
                    let _ = view.update(cx, |view, cx| view.follow(href, cx));
                })
                .into_any_element();
        }
        let view = cx.entity().downgrade();
        let scroll = self.scroll.clone();
        let progress = if self.book.format() == Format::Comic {
            (self.state.chapter + 1) as f32 / self.book.chapters().len() as f32
        } else {
            (self.state.chapter as f32 + self.state.fraction) / self.book.chapters().len() as f32
        };
        let metadata = h_flex().p_3().child(
            v_flex()
                .id("book-metadata")
                .on_double_click(cx.listener(|view, _, _, cx| {
                    cx.emit(Request::ShowText {
                        title: "Book metadata",
                        text: format!("{}\n\n{}", view.book.title(), view.book.author()),
                    });
                }))
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(self.excerpt(self.book.title())),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(format!(
                            "{} · {}",
                            self.book.format().label(),
                            self.excerpt(self.book.author())
                        )),
                ),
        );
        let controls = super::super::controls::ActionBindings::new()
            .action(
                "book-contents",
                crate::views::icon_button(
                    "book-contents",
                    gpui_kit::assets::IconName::List,
                    tr("Contents"),
                )
                .selected(self.contents)
                .on_click(cx.listener(|v, _, _, cx| {
                    (v.host.action)("ebook", "toggle_contents", &[]);
                    v.contents = !v.contents;
                    v.bookmarks = false;
                    cx.notify();
                })),
            )
            .action(
                "font-smaller",
                Button::new("book-font-smaller")
                    .small()
                    .label("A−")
                    .disabled(font <= 14.)
                    .on_click(cx.listener(|v, _, _, cx| {
                        (v.host.action)("ebook", "font_smaller", &[]);
                        v.state.font_size = (v.state.font_size - 2.).max(14.);
                        v.save(cx);
                        cx.notify();
                    })),
            )
            .action("font-status", div().text_xs().child(format!("{font:.0}")))
            .action(
                "font-larger",
                Button::new("book-font-larger")
                    .small()
                    .label("A+")
                    .disabled(font >= 32.)
                    .on_click(cx.listener(|v, _, _, cx| {
                        (v.host.action)("ebook", "font_larger", &[]);
                        v.state.font_size = (v.state.font_size + 2.).min(32.);
                        v.save(cx);
                        cx.notify();
                    })),
            )
            .action(
                "font-family",
                Button::new("book-font")
                    .small()
                    .label(tr(if self.state.serif {
                        "Serif"
                    } else {
                        "Sans serif"
                    }))
                    .on_click(cx.listener(|v, _, _, cx| {
                        (v.host.action)("ebook", "font_family", &[]);
                        v.state.serif = !v.state.serif;
                        v.save(cx);
                        cx.notify();
                    })),
            )
            .action(
                "line-spacing",
                Button::new("book-spacing")
                    .small()
                    .label(format!("{} {:.1}", tr("Spacing"), self.state.line_height))
                    .on_click(cx.listener(|v, _, _, cx| {
                        (v.host.action)("ebook", "line_spacing", &[]);
                        v.state.line_height = if v.state.line_height < 1.6 {
                            1.7
                        } else if v.state.line_height < 1.9 {
                            2.0
                        } else {
                            1.5
                        };
                        v.save(cx);
                        cx.notify();
                    })),
            )
            .action(
                "page-width",
                Button::new("book-width")
                    .small()
                    .label(tr("Page width"))
                    .on_click(cx.listener(|v, _, _, cx| {
                        (v.host.action)("ebook", "page_width", &[]);
                        v.state.page_width = if v.state.page_width < 650. {
                            720.
                        } else if v.state.page_width < 800. {
                            880.
                        } else {
                            560.
                        };
                        v.save(cx);
                        cx.notify();
                    })),
            )
            .action(
                "paper",
                Button::new("book-paper")
                    .small()
                    .selected(self.state.paper)
                    .label(tr("Paper"))
                    .on_click(cx.listener(|v, _, _, cx| {
                        (v.host.action)("ebook", "paper_style", &[]);
                        v.state.paper = !v.state.paper;
                        v.save(cx);
                        cx.notify();
                    })),
            )
            .action(
                "bookmark",
                crate::views::icon_button(
                    "book-bookmark",
                    gpui_kit::assets::IconName::Bookmark,
                    tr(if marked {
                        "Remove bookmark"
                    } else {
                        "Add bookmark"
                    }),
                )
                .selected(marked)
                .on_click(cx.listener(|v, _, _, cx| v.bookmark(cx))),
            )
            .action(
                "previous",
                crate::views::icon_button(
                    "book-previous",
                    gpui_kit::assets::IconName::ChevronLeft,
                    tr("Previous page"),
                )
                .on_click(cx.listener(|v, _, _, cx| v.page(false, cx))),
            )
            .action(
                "page-status",
                div()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{} / {} · {} · {:.0}%",
                        self.state.chapter + 1,
                        self.book.chapters().len(),
                        self.book.chapters()[self.state.chapter].title,
                        progress * 100.
                    )),
            )
            .action(
                "next",
                crate::views::icon_button(
                    "book-next",
                    gpui_kit::assets::IconName::ChevronRight,
                    tr("Next page"),
                )
                .on_click(cx.listener(|v, _, _, cx| v.page(true, cx))),
            );
        v_flex()
            .relative()
            .size_full()
            .min_h_0()
            .track_focus(&self.focus)
            .key_context("Ebook")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _, window, cx| window.focus(&view.focus, cx)),
            )
            .on_action(cx.listener(|view, _: &BookNextPage, _, cx| view.page(true, cx)))
            .on_action(cx.listener(|view, _: &BookPreviousPage, _, cx| view.page(false, cx)))
            .on_action(cx.listener(|view, _: &BookNextChapter, _, cx| view.chapter(true, cx)))
            .on_action(cx.listener(|view, _: &BookPreviousChapter, _, cx| view.chapter(false, cx)))
            .on_action(cx.listener(|view, _: &BookContents, _, cx| {
                view.contents = !view.contents;
                cx.notify();
            }))
            .child(metadata)
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .when(self.contents, |row| {
                        row.child(
                            v_flex()
                                .w(px(224.))
                                .h_full()
                                .flex_shrink_0()
                                .border_r_1()
                                .border_color(cx.theme().border)
                                .child(
                                    h_flex()
                                        .p_2()
                                        .gap_2()
                                        .child(
                                            Button::new("book-toc-tab")
                                                .ghost()
                                                .small()
                                                .selected(!self.bookmarks)
                                                .label(tr("Contents"))
                                                .on_click(cx.listener(|v, _, _, cx| {
                                                    v.bookmarks = false;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            Button::new("book-marks-tab")
                                                .ghost()
                                                .small()
                                                .selected(self.bookmarks)
                                                .label(tr("Bookmarks"))
                                                .on_click(cx.listener(|v, _, _, cx| {
                                                    v.bookmarks = true;
                                                    cx.notify();
                                                })),
                                        ),
                                )
                                .child(
                                    uniform_list(
                                        "book-toc",
                                        if self.bookmarks {
                                            self.state.bookmarks.len()
                                        } else {
                                            self.book.toc().len()
                                        },
                                        cx.processor(move |v, range: std::ops::Range<usize>, _, cx| {
                                            range
                                                .map(|i| {
                                                    let (title, href, depth, bookmark) =
                                                        if v.bookmarks {
                                                            let b = &v.state.bookmarks[i];
                                                            (
                                                                b.label.clone(),
                                                                String::new(),
                                                                0,
                                                                Some((b.chapter, b.fraction)),
                                                            )
                                                        } else {
                                                            let t = &v.book.toc()[i];
                                                            (
                                                                t.title.clone(),
                                                                t.href.clone(),
                                                                t.depth,
                                                                None,
                                                            )
                                                        };
                                                    let label = v.excerpt(&title);
                                                    let shortened = label != title;
                                                    let full = title.clone();
                                                    let entry = Button::new(("book-toc-item", i))
                                                        .ghost()
                                                        .label(label.clone())
                                                        .tooltip(label)
                                                        .flex_1().min_w_0()
                                                        .h(px(36.))
                                                        .justify_start()
                                                        .pl(px(12. + depth.min(v.policy.toc_depth) as f32 * 12.))
                                                        .on_click(cx.listener(
                                                            move |v, _, _, cx| {
                                                                if let Some((chapter, fraction)) =
                                                                    bookmark
                                                                {
                                                                    v.go(
                                                                        chapter, fraction, None, cx,
                                                                    );
                                                                } else {
                                                                    v.follow(&href, cx);
                                                                }
                                                            },
                                                        ))
;
                                                    h_flex().w_full().min_w_0().child(entry)
                                                        .when(shortened, |row| row.child(Button::new(("book-full-label", i)).ghost().small().label("…")
                                                            .tooltip(tr("Full text")).on_click(cx.listener(move |_, _, _, cx| {
                                                                cx.emit(Request::ShowText { title: "Full text", text: full.clone() });
                                                            }))))
                                                        .into_any_element()
                                                })
                                                .collect()
                                        }),
                                    )
                                    .flex_1()
                                    .min_h_0()
                                    .track_scroll(&self.toc_scroll),
                                ),
                        )
                    })
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .bg(cx.theme().muted)
                            .child(
                                div()
                                    .id("ebook-scroll")
                                    .size_full()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.scroll)
                                    .child(
                                        v_flex().w_full().items_center().pt_6().pb(px(128.)).child(
                                            div()
                                                .w_full()
                                                .max_w(px(self.state.page_width))
                                                .min_h(px(300.))
                                                .px_8()
                                                .py_7()
                                                .bg(paper)
                                                .rounded_md()
                                                .shadow_sm()
                                                .child(content),
                                        ),
                                    )
                                    .child(
                                        canvas(
                                            |_, _, _| (),
                                            move |_, _, window, cx| {
                                                let view = view.clone();
                                                let scroll = scroll.clone();
                                                window.defer(cx, move |_, cx| {
                                                    let _ = view.update(cx, |v, cx| {
                                                        let ready = v.active && (v.comic.is_some()
                                                            || v.text.as_ref().is_some_and(|t| {
                                                                !t.read(cx)
                                                                    .rendered_text()
                                                                    .is_empty()
                                                            }));
                                                        if ready
                                                            && let Some(fraction) = v.restore.take()
                                                        {
                                                            scroll.set_offset(point(
                                                                px(0.),
                                                                -scroll.max_offset().y * fraction,
                                                            ));
                                                            cx.notify();
                                                        }
                                                        if ready
                                                            && let Some(anchor) = v.anchor.take()
                                                            && let (Some(text), Some(source)) =
                                                                (&v.text, v.anchors.get(&anchor))
                                                            && let Some(range) = text
                                                                .read(cx)
                                                                .rendered_text()
                                                                .range_for_source(source.clone())
                                                        {
                                                            text.update(cx, |text, cx| {
                                                                let _ =
                                                                    text.reveal_range(range, cx);
                                                            });
                                                        }
                                                        v.position(cx);
                                                    });
                                                });
                                            },
                                        )
                                        .absolute()
                                        .size_full(),
                                    ),
                            )
                            .when(self.paged.is_none(), |view| {
                                view.child(Scrollbar::vertical(&self.scroll))
                            })
                            .when_some(
                                self.paged
                                    .clone()
                                    .filter(|_| !self.loading && self.error.is_none()),
                                |view, page| {
                                    view.child(
                                        div()
                                            .absolute()
                                            .size_full()
                                            .bg(cx.theme().background)
                                            .child(page),
                                    )
                                },
                            ),
                    ),
            )
            .child(crate::views::reader::controls::floating("book-controls", controls, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BookView, Catalog, Chapter, Format, Policy, PreparedChapter, ReaderHost, Request, TocEntry,
    };
    use glim_core::resources::{ResourceClass, ResourcePool};
    use gpui_kit::{AppContext, Entity, TestAppContext, WindowOptions};
    use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

    struct Fixture {
        chapters: Vec<Chapter>,
        toc: Vec<TocEntry>,
    }
    impl Catalog for Fixture {
        fn title(&self) -> &str {
            "目录与状态契约"
        }
        fn author(&self) -> &str {
            "Reader Fixture"
        }
        fn format(&self) -> Format {
            Format::FictionBook
        }
        fn chapters(&self) -> &[Chapter] {
            &self.chapters
        }
        fn toc(&self) -> &[TocEntry] {
            &self.toc
        }
    }
    fn open(cx: &mut TestAppContext, pool: &ResourcePool) -> (Arc<Fixture>, Entity<BookView>) {
        cx.update(gpui_kit::init);
        let catalog = Arc::new(Fixture {
            chapters: vec![
                Chapter {
                    href: "chapter-0".into(),
                    title: "第一章".into(),
                    html: None,
                },
                Chapter {
                    href: "chapter-1".into(),
                    title: "第二章".into(),
                    html: None,
                },
            ],
            toc: vec![TocEntry {
                title: "目录一".into(),
                href: "chapter-0#first".into(),
                depth: 1,
            }],
        });
        let (_, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    BookView::new(
                        catalog.clone(),
                        ReaderHost {
                            translate: str::to_owned,
                            action: |_, _, _| {},
                            resources: pool.clone(),
                        },
                        Policy {
                            label_chars: 4,
                            toc_depth: 3,
                        },
                        pool.track(100, ResourceClass::State),
                        window,
                        cx,
                    )
                })
            })
            .unwrap()
        });
        (catalog, view)
    }
    fn content(pool: &ResourcePool) -> PreparedChapter {
        PreparedChapter {
            html: "<p id=\"start\">Alpha 中文正文。</p>".into(),
            anchors: HashMap::from([("start".into(), 0..12)]),
            images: Arc::new(|_| "unused".into()),
            comic: None,
            comic_ratio: 1.,
            page: None,
            memory: pool.track(500, ResourceClass::Working),
        }
    }
    #[gpui_kit::test]
    fn prepared_content_retains_catalog_ownership_selection_and_releases_unselected_layout(
        cx: &mut TestAppContext,
    ) {
        let pool = ResourcePool::with_budgets(0, 4096);
        let (catalog, view) = open(cx, &pool);
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                assert_eq!(view.book.chapters().as_ptr(), catalog.chapters.as_ptr());
                assert_eq!(view.excerpt("中文标题很长"), "中文标题…");
                view.state.fraction = 0.625;
                view.install(content(&pool), cx);
            })
        });
        cx.run_until_parked();
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                let text = view.text.clone().unwrap();
                assert!(!text.read(cx).rendered_text().is_empty());
                text.update(cx, |text, cx| text.select_all(cx));
                assert!(!text.read(cx).selected_text().is_empty());
                let entity = text.entity_id();
                assert!(view.suspend(cx));
                assert_eq!(view.text.as_ref().unwrap().entity_id(), entity);
                assert_eq!(pool.snapshot().working_bytes, 500);
                view.active = true;
                text.update(cx, |text, cx| text.clear_selection(cx));
                assert!(!view.suspend(cx));
                assert!(!view.has_content());
                assert_eq!(view.state.fraction, 0.625);
                assert_eq!(pool.snapshot().working_bytes, 0);
                assert_eq!(pool.snapshot().state_bytes, 100);
            })
        });
    }
    #[gpui_kit::test]
    fn navigation_bookmarks_and_persistence_are_host_requests(cx: &mut TestAppContext) {
        let pool = ResourcePool::with_budgets(0, 4096);
        let (_, view) = open(cx, &pool);
        let requests = Rc::new(RefCell::new(Vec::new()));
        let received = requests.clone();
        let _subscription = cx.update(|cx| {
            cx.subscribe(&view, move |_, request: &Request, _| {
                received.borrow_mut().push(request.clone())
            })
        });
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                view.go(1, 0.25, Some("target".into()), cx);
                view.bookmark(cx);
                view.follow("chapter-0#first", cx);
                view.suspend(cx);
                assert_eq!(
                    view.pending.lock().unwrap().as_ref().unwrap().bookmarks[0].label,
                    "第二章"
                );
                assert_eq!(view.anchor.as_deref(), Some("target"));
                assert_eq!(view.revision, 1);
            })
        });
        cx.run_until_parked();
        let requests = requests.borrow();
        assert!(requests.iter().any(|r| matches!(r, Request::Prepare(1))));
        assert!(requests.iter().any(|r| matches!(r, Request::Persist)));
        assert!(
            requests
                .iter()
                .any(|r| matches!(r, Request::Follow(href) if href == "chapter-0#first"))
        );
    }
}
