use super::super::minimap::Minimap;
use crate::app::reader_jobs::{self, JobClass, Priority};
use gpui_kit::{
    component::{
        ActiveTheme, WindowExt,
        input::{Editor, EditorState, InputEvent},
    },
    *,
};
use std::rc::Rc;

/// Shared basic editor surface for text/code and read-only value dialogs.
/// Native gutter, folding and scrollbars share the editor's layout frame.
pub(super) struct SourceReader {
    pub edit: crate::app::edit_session::EditSession,
    pub minimap: Entity<Minimap>,
    _minimap_subscription: Subscription,
}
impl SourceReader {
    pub fn new(
        text: &str,
        language: &'static str,
        readonly: bool,
        scroll: ScrollHandle,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let state = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .language(language)
                .auto_close(false)
                .smart_indent(false)
                .line_number(!readonly)
                .folding(!readonly)
                .soft_wrap(false)
                .scroll_beyond_last_line(Some(0));
            if super::lexical::supports(language) {
                state.set_highlighter_factory(
                    Rc::new(|name| Some(Box::new(super::lexical::Lexical::new(name)))),
                    cx,
                );
            }
            state.set_value(text.to_owned(), window, cx);
            state.set_readonly(readonly, cx);
            state
        });
        let minimap = cx.new(|cx| Minimap::new(state.clone(), text, scroll, cx));
        let edit = crate::app::edit_session::EditSession::new(state.clone(), text, readonly, cx);
        let map = minimap.clone();
        let subscription = cx.subscribe(&state, move |state, event, cx| {
            if matches!(event, InputEvent::Change) {
                let text = state.read(cx).value().to_string();
                map.update(cx, |map, cx| map.set_text(&text, cx));
            }
        });
        Self {
            edit,
            minimap,
            _minimap_subscription: subscription,
        }
    }
    pub fn is_dirty(&self) -> bool {
        self.edit.is_dirty()
    }
    pub fn mark_saved(&mut self, text: String, cx: &App) {
        self.edit.mark_saved(text, cx);
    }
    pub fn inherit(&mut self, old: &Self, cx: &mut App) {
        let offset = old.edit.editor().read(cx).scroll_offset();
        self.edit
            .editor()
            .update(cx, |s, cx| s.set_scroll_offset(offset, cx));
    }
    pub fn render(&mut self, cx: &mut App) -> AnyElement {
        // The toolkit invokes custom menus while updating EditorState. Capture
        // capabilities during rendering instead of re-entering that entity.
        let capabilities = self.edit.editor().read(cx).context_menu_capabilities();
        let menu_editable = capabilities.is_editable();
        let menu_copyable = capabilities.is_copyable();
        self.minimap
            .update(cx, |map, cx| map.set_preview(false, cx));
        gpui_kit::component::h_flex()
            .size_full()
            .font_family("Menlo")
            .text_size(px(13.))
            .line_height(px(20.))
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .child(
                        Editor::new(self.edit.editor())
                            .context_menu(move |menu, _, _| {
                                super::editor_menu(menu, menu_editable, menu_copyable)
                            })
                            .readonly(self.edit.is_readonly() || self.edit.is_locked())
                            .appearance(false)
                            .bordered(false)
                            .size_full(),
                    )
                    // Hide the duplicate vertical rail, not the horizontal scroll track.
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .right_0()
                            .w(px(12.))
                            .bg(cx.theme().background),
                    ),
            )
            .child(
                div()
                    .w(px(110.))
                    .flex_shrink_0()
                    .h_full()
                    .child(self.minimap.clone()),
            )
            .into_any_element()
    }
}

struct TextDialog {
    source: Option<SourceReader>,
    page: Option<Entity<super::paged::PagedReader>>,
    task: Option<Task<()>>,
    error: Option<String>,
}
impl TextDialog {
    fn new(text: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        if text.len() <= glim_services::limits::TEXT_LINE_BYTES && !text.contains('\0') {
            return Self {
                source: Some(SourceReader::new(
                    &text,
                    "plain",
                    true,
                    ScrollHandle::new(),
                    window,
                    cx,
                )),
                page: None,
                task: None,
                error: None,
            };
        }
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let work = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Projection,
            Priority::Interactive,
            cancel,
            move |cancel| glim_services::paged::text_value(&text, cancel),
        );
        let task = cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                view.task = None;
                match result {
                    Ok(page) => {
                        view.page = Some(cx.new(|_| super::paged::PagedReader::text_only(page)))
                    }
                    Err(error) => view.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        });
        Self {
            source: None,
            page: None,
            task: Some(task),
            error: None,
        }
    }
}
impl Render for TextDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(source) = &mut self.source {
            return source.render(cx);
        }
        if let Some(page) = &self.page {
            return page.clone().into_any_element();
        }
        if let Some(error) = &self.error {
            return div().p_3().child(error.clone()).into_any_element();
        }
        super::preview_loading(cx)
    }
}
pub(super) fn show_text_dialog(title: String, text: String, window: &mut Window, cx: &mut App) {
    let content = cx.new(|cx| TextDialog::new(text, window, cx));
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(title.clone())
            .width(px(640.))
            .child(div().h(px(360.)).child(content.clone()))
    });
}

#[cfg(test)]
mod format_registry_tests {
    #[test]
    fn new_source_formats_have_compiled_grammars() {
        crate::app::languages::init();
        let registry = gpui_kit::component::highlighter::LanguageRegistry::singleton();
        for name in [
            "java", "sql", "make", "html", "c", "cpp", "python", "json", "ruby", "php", "csharp",
            "swift", "kotlin", "lua", "scala", "elixir", "zig", "graphql", "proto", "cmake",
            "astro", "svelte", "erb", "ejs", "jsdoc",
        ] {
            let grammar = registry
                .language(name)
                .expect("enabled grammar must be registered");
            assert!(grammar.language.is_some(), "{name}");
            assert!(!grammar.highlights.is_empty(), "{name}");
            let highlighter = gpui_kit::component::highlighter::SyntaxHighlighter::new(name);
            assert_eq!(
                highlighter.language().as_ref(),
                name,
                "invalid query for {name}"
            );
        }
    }
}
