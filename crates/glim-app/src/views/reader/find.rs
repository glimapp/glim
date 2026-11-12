use glim_services::locale::tr;
use gpui_kit::{
    assets::IconName,
    base::{RangeHighlight, RenderedText, TextViewState, input::SearchMatcher},
    component::{
        ActiveTheme, Disableable, Icon, Sizable,
        button::{Button, ButtonCustomVariant, ButtonVariants},
        h_flex,
        input::{EditorState, Escape, Input, InputEvent, InputState, Replace, Search},
        tooltip::Tooltip,
        v_flex,
    },
    prelude::FluentBuilder,
    *,
};

actions!(reader_find, [NextMatch, PreviousMatch]);

#[derive(Clone)]
pub(super) enum Target {
    Editor(Entity<EditorState>),
    Preview(Entity<TextViewState>),
}

pub(super) struct FindBar {
    target: Target,
    input: Entity<InputState>,
    replacement: Entity<InputState>,
    open: bool,
    replace: bool,
    case_sensitive: bool,
    whole_word: bool,
    regex: bool,
    error: bool,
    matcher: SearchMatcher,
    rendered: Option<RenderedText>,
    pending: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl FindBar {
    pub fn new(target: Target, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(tr("Find")));
        let replacement = cx.new(|cx| InputState::new(window, cx).placeholder(tr("Replace")));
        let mut subscriptions =
            vec![
                cx.subscribe_in(&input, window, |view, _, event, _, cx| match event {
                    InputEvent::Change => view.schedule(cx),
                    InputEvent::PressEnter { shift, .. } => view.step(*shift, cx),
                    _ => {}
                }),
            ];
        subscriptions.push(
            cx.subscribe_in(&replacement, window, |view, _, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    view.replace_matches(false, window, cx);
                }
            }),
        );
        match &target {
            Target::Editor(editor) => {
                subscriptions.push(cx.subscribe(editor, |view, _, event, cx| {
                    if view.open && matches!(event, InputEvent::Change) {
                        view.schedule(cx);
                    }
                }))
            }
            Target::Preview(text) => subscriptions.push(cx.observe(text, |view, text, cx| {
                if view.open && view.rendered.as_ref() != Some(&text.read(cx).rendered_text()) {
                    view.schedule(cx);
                }
            })),
        }
        Self {
            target,
            input,
            replacement,
            open: false,
            replace: false,
            case_sensitive: false,
            whole_word: false,
            regex: false,
            error: false,
            matcher: SearchMatcher::new(),
            rendered: None,
            pending: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn show(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        let selected = match &self.target {
            Target::Editor(editor) => editor.read(cx).selected_text().to_string(),
            Target::Preview(text) => text.read(cx).selected_text(),
        };
        self.open = true;
        self.replace = replace && self.replaceable(cx);
        self.record(
            "find_opened",
            [("replace", glim_services::telemetry::flag(self.replace))],
        );
        self.input.update(cx, |input, cx| {
            if !selected.is_empty() && !selected.contains('\n') {
                input.set_value(selected, window, cx);
            }
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        self.refresh(true, cx);
    }

    pub fn current_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.set_placeholder(tr("Find in current page"), window, cx)
        });
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        self.pending = None;
        match &self.target {
            Target::Editor(editor) => editor.update(cx, |editor, cx| {
                editor.close_search(cx);
                editor.focus(window, cx);
            }),
            Target::Preview(text) => text.update(cx, |text, cx| {
                text.clear_range_highlights(cx);
                text.focus_handle().focus(window, cx);
            }),
        }
        cx.notify();
    }

    fn schedule(&mut self, cx: &mut Context<Self>) {
        self.pending = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(120))
                .await;
            let _ = view.update(cx, |view, cx| view.refresh(true, cx));
        }));
        cx.notify();
    }

    fn refresh(&mut self, reveal: bool, cx: &mut Context<Self>) {
        self.pending = None;
        if !self.open {
            return;
        }
        let query = self.input.read(cx).value();
        match &self.target {
            Target::Editor(editor) => {
                self.error = editor.update(cx, |editor, cx| {
                    let result = editor.set_search_options(
                        &query,
                        self.case_sensitive,
                        self.whole_word,
                        self.regex,
                        cx,
                    );
                    if reveal {
                        editor.reveal_current_search_match(cx);
                    }
                    result.is_err()
                });
            }
            Target::Preview(text) => {
                let rendered = text.read(cx).rendered_text();
                self.matcher.update(&rendered.as_str().into());
                self.error = self
                    .matcher
                    .update_options(&query, self.case_sensitive, self.whole_word, self.regex)
                    .is_err();
                self.rendered = Some(rendered);
                self.paint_preview(reveal, cx);
            }
        }
        cx.notify();
    }

    fn paint_preview(&self, reveal: bool, cx: &mut Context<Self>) {
        let Target::Preview(text) = &self.target else {
            return;
        };
        let ranges = self.matcher.matched_ranges();
        let current = self.matcher.current();
        let color = cx.theme().primary;
        text.update(cx, |text, cx| {
            let _ = text.set_range_highlights(
                ranges.iter().enumerate().map(|(index, range)| {
                    RangeHighlight::new(
                        range.clone(),
                        color.opacity(if Some(index) == current { 0.42 } else { 0.18 }),
                    )
                }),
                cx,
            );
            if reveal && let Some(range) = current.and_then(|index| ranges.get(index)) {
                let _ = text.reveal_range(range.clone(), cx);
            }
        });
    }

    fn step(&mut self, previous: bool, cx: &mut Context<Self>) {
        // Enter during the debounce searches the new query before navigating.
        if self.pending.is_some() {
            self.refresh(true, cx);
            return;
        }
        match &self.target {
            Target::Editor(editor) => editor.update(cx, |editor, cx| {
                if previous {
                    editor.previous_search_match(cx);
                } else {
                    editor.next_search_match(cx);
                }
                cx.notify();
            }),
            Target::Preview(_) => {
                if previous {
                    self.matcher.next_back();
                } else {
                    self.matcher.next();
                }
                self.paint_preview(true, cx);
            }
        }
        cx.notify();
    }

    fn replaceable(&self, cx: &App) -> bool {
        matches!(&self.target, Target::Editor(editor) if editor.read(cx).is_replaceable())
    }

    fn replace_matches(&mut self, all: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending.is_some() {
            self.refresh(false, cx);
        }
        if self.error {
            return;
        }
        if let Target::Editor(editor) = &self.target {
            if !self.replaceable(cx) {
                return;
            }
            self.record("replace", [("all", glim_services::telemetry::flag(all))]);
            let replacement = self.replacement.read(cx).value();
            editor.update(cx, |editor, cx| {
                if all {
                    editor.replace_all_search_matches(&replacement, window, cx);
                } else {
                    editor.replace_current_search_match(&replacement, window, cx);
                }
            });
        }
        cx.notify();
    }

    fn record(
        &self,
        action: &'static str,
        extra: impl IntoIterator<Item = (&'static str, glim_services::telemetry::Property)>,
    ) {
        glim_services::telemetry::reader_feature(
            match &self.target {
                Target::Editor(_) => "source",
                Target::Preview(_) => "text_preview",
            },
            action,
            extra,
        );
    }

    fn control(id: &'static str, label: &str, icon: IconName) -> Button {
        Button::new(id)
            .ghost()
            .small()
            .w(px(28.))
            .h(px(28.))
            .icon(Icon::new(icon).size(px(18.)))
            .tooltip(tr(label))
            .accessibility_label(tr(label))
    }

    fn option(id: &'static str, label: &str, icon: IconName, enabled: bool, cx: &App) -> Button {
        let accent = crate::views::accent(cx);
        let background = if enabled {
            cx.theme().popover.blend(accent.opacity(0.20))
        } else {
            cx.theme().transparent
        };
        Self::control(id, label, icon)
            .toggled(enabled)
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(background)
                    .foreground(if enabled {
                        accent
                    } else {
                        cx.theme().muted_foreground
                    })
                    .hover(if enabled {
                        background.blend(accent.opacity(0.08))
                    } else {
                        cx.theme().muted
                    })
                    .active(accent.opacity(0.28)),
            )
            .bg(background)
            .border_1()
            .border_color(if enabled {
                accent.opacity(0.65)
            } else {
                cx.theme().transparent
            })
    }
}

impl Render for FindBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return Empty.into_any_element();
        }
        let matcher = match &self.target {
            Target::Editor(editor) => &editor.read(cx).search_session().matcher,
            Target::Preview(_) => &self.matcher,
        };
        let label = matcher.label();
        let empty = matcher.is_empty() || self.pending.is_some() || self.error;
        let query_empty = self.input.read(cx).value().is_empty();
        let status = if self.error {
            div()
                .id("find-error")
                .flex()
                .justify_center()
                .child(Icon::new(IconName::CircleAlert).size(px(18.)))
                .tooltip(|window, cx| {
                    Tooltip::new(tr("Invalid regular expression")).build(window, cx)
                })
                .role(Role::Status)
                .aria_label(tr("Invalid regular expression"))
                .into_any_element()
        } else {
            div()
                .id("find-count")
                .truncate()
                .child(if query_empty {
                    String::new()
                } else if self.pending.is_some() {
                    "…".to_owned()
                } else {
                    label.clone()
                })
                .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
                .into_any_element()
        };
        let options = h_flex()
            .flex_shrink_0()
            .gap(px(1.))
            .child(
                Self::option(
                    "find-case",
                    "Match case",
                    IconName::CaseSensitive,
                    self.case_sensitive,
                    cx,
                )
                .on_click(cx.listener(|view, _, _, cx| {
                    view.case_sensitive = !view.case_sensitive;
                    view.refresh(true, cx);
                })),
            )
            .child(
                Self::option(
                    "find-word",
                    "Match whole word",
                    IconName::WholeWord,
                    self.whole_word,
                    cx,
                )
                .on_click(cx.listener(|view, _, _, cx| {
                    view.whole_word = !view.whole_word;
                    view.refresh(true, cx);
                })),
            )
            .child(
                Self::option(
                    "find-regex",
                    "Use regular expression",
                    IconName::Regex,
                    self.regex,
                    cx,
                )
                .on_click(cx.listener(|view, _, _, cx| {
                    view.regex = !view.regex;
                    view.refresh(true, cx);
                })),
            );
        let controls = h_flex()
            .flex_shrink_0()
            .gap(px(1.))
            .child(
                Self::control(
                    "find-prev",
                    "Previous match · Shift+Enter",
                    IconName::ArrowUp,
                )
                .disabled(empty)
                .on_click(cx.listener(|view, _, _, cx| view.step(true, cx))),
            )
            .child(
                Self::control("find-next", "Next match · Enter", IconName::ArrowDown)
                    .disabled(empty)
                    .on_click(cx.listener(|view, _, _, cx| view.step(false, cx))),
            )
            .child(options);
        let panel = v_flex()
            .id("reader-find")
            .w(px(400.))
            .max_w(relative(1.))
            .p(px(6.))
            .gap(px(4.))
            .occlude()
            .bg(cx.theme().popover)
            .text_color(cx.theme().foreground)
            .font_family(cx.theme().font_family.clone())
            .text_size(px(13.))
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(6.))
            .shadow_sm()
            .on_action(cx.listener(|view, _: &Escape, window, cx| view.close(window, cx)))
            .on_action(cx.listener(|view, _: &Search, window, cx| view.show(false, window, cx)))
            .on_action(cx.listener(|view, _: &Replace, window, cx| view.show(true, window, cx)))
            .on_action(cx.listener(|view, _: &NextMatch, _, cx| view.step(false, cx)))
            .on_action(cx.listener(|view, _: &PreviousMatch, _, cx| view.step(true, cx)))
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .gap(px(4.))
                    .when(self.replaceable(cx), |row| {
                        row.child(
                            Self::control(
                                "find-replace-toggle",
                                "Toggle replace",
                                if self.replace {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                },
                            )
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.replace = !view.replace;
                                cx.notify();
                            })),
                        )
                    })
                    .child(
                        div().flex_1().min_w(px(100.)).child(
                            Input::new(&self.input)
                                .small()
                                .appearance(true)
                                .bordered(true)
                                .focus_bordered(false)
                                .bg(cx.theme().popover)
                                .suffix(
                                    div()
                                        .w(px(42.))
                                        .flex_shrink_0()
                                        .text_center()
                                        .text_color(
                                            if self.error
                                                || (matcher.is_empty()
                                                    && !query_empty
                                                    && self.pending.is_none())
                                            {
                                                cx.theme().danger
                                            } else {
                                                cx.theme().muted_foreground
                                            },
                                        )
                                        .child(status),
                                ),
                        ),
                    )
                    .child(controls)
                    .child(
                        Self::control("find-close", "Close find · Esc", IconName::X)
                            .on_click(cx.listener(|view, _, window, cx| view.close(window, cx))),
                    ),
            )
            .when(self.replace && self.replaceable(cx), |panel| {
                panel.child(
                    h_flex()
                        .gap(px(4.))
                        .child(div().w(px(28.)))
                        .child(
                            div().flex_1().min_w_0().child(
                                Input::new(&self.replacement)
                                    .small()
                                    .appearance(true)
                                    .bordered(true)
                                    .focus_bordered(false)
                                    .bg(cx.theme().popover),
                            ),
                        )
                        .child(
                            h_flex()
                                .w(px(176.))
                                .max_w(relative(0.5))
                                .flex_shrink_0()
                                .gap(px(4.))
                                .child(
                                    Self::control("find-replace-one", "Replace", IconName::Replace)
                                        .disabled(empty)
                                        .on_click(cx.listener(|view, _, window, cx| {
                                            view.replace_matches(false, window, cx)
                                        })),
                                )
                                .child(
                                    Self::control(
                                        "find-replace-all",
                                        "Replace all",
                                        IconName::ReplaceAll,
                                    )
                                    .disabled(empty)
                                    .on_click(cx.listener(
                                        |view, _, window, cx| {
                                            view.replace_matches(true, window, cx)
                                        },
                                    )),
                                ),
                        ),
                )
            })
            .into_any_element();
        div()
            .absolute()
            .top(px(12.))
            .left(px(12.))
            .right(px(12.))
            .flex()
            .justify_end()
            .child(panel)
            .into_any_element()
    }
}

pub(super) fn wrap(
    content: impl IntoElement,
    find: &Entity<FindBar>,
    trailing_inset: f32,
) -> AnyElement {
    let search = find.clone();
    let replace = find.clone();
    let next = find.clone();
    let previous = find.clone();
    let close = find.clone();
    div()
        .relative()
        .size_full()
        .on_action(move |_: &Search, window, cx| {
            search.update(cx, |view, cx| view.show(false, window, cx))
        })
        .on_action(move |_: &Replace, window, cx| {
            replace.update(cx, |view, cx| view.show(true, window, cx))
        })
        .on_action(move |_: &Escape, window, cx| {
            close.update(cx, |view, cx| {
                if view.open {
                    view.close(window, cx);
                } else {
                    cx.propagate();
                }
            })
        })
        .on_action(move |_: &NextMatch, _, cx| {
            next.update(cx, |view, cx| {
                if view.open {
                    view.step(false, cx);
                } else {
                    cx.propagate();
                }
            })
        })
        .on_action(move |_: &PreviousMatch, _, cx| {
            previous.update(cx, |view, cx| {
                if view.open {
                    view.step(true, cx);
                } else {
                    cx.propagate();
                }
            })
        })
        .child(content)
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .right(px(trailing_inset))
                .child(find.clone()),
        )
        .into_any_element()
}
