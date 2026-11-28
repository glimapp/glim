use super::*;

impl Reader {
    pub fn can_find(&self, cx: &App) -> bool {
        if self.projection.as_ref().is_some_and(Projection::visible) {
            return self
                .projection
                .as_ref()
                .is_some_and(|view| view.find(false, cx));
        }
        match &self.renderer {
            Renderer::Surface(view) => view.can_find(cx),
            Renderer::Markup(m) if m.preview && m.html => m
                .browser
                .as_ref()
                .is_some_and(|view| view.read(cx).find(false)),
            Renderer::Paged { view, html, .. } => {
                match html.as_ref().and_then(PagedHtml::preview) {
                    Some(browser) => browser.read(cx).find(false),
                    None => view.read(cx).source_editor.is_some(),
                }
            }
            _ => self.source().is_some(),
        }
    }

    pub fn find(&self, window: &mut Window, cx: &mut App) {
        self.show_find(false, false, window, cx);
    }
    pub fn toggle_find(&self, window: &mut Window, cx: &mut App) {
        self.show_find(false, true, window, cx);
    }
    pub fn replace(&self, window: &mut Window, cx: &mut App) {
        if self.mode() == glim_core::ReaderMode::Source
            && self
                .editor()
                .is_some_and(|editor| editor.read(cx).is_replaceable())
        {
            self.show_find(true, false, window, cx);
        }
    }
    fn show_find(&self, replace: bool, toggle: bool, window: &mut Window, cx: &mut App) {
        if !self.can_find(cx) {
            return;
        }
        if let Some(view) = &self.projection
            && view.visible()
        {
            if toggle {
                view.toggle_find(cx);
            } else {
                view.find(true, cx);
            }
            return;
        }
        match &self.renderer {
            Renderer::Surface(view) => {
                if toggle {
                    view.toggle_find(window, cx);
                } else {
                    view.find(window, cx);
                }
                return;
            }
            Renderer::Markup(m) if m.preview && m.html => {
                if let Some(view) = &m.browser {
                    if toggle {
                        view.read(cx).toggle_find();
                    } else {
                        view.read(cx).find(true);
                    }
                }
                return;
            }
            Renderer::Paged { view, html, .. } => {
                if let Some(browser) = html.as_ref().and_then(PagedHtml::preview) {
                    if toggle {
                        browser.read(cx).toggle_find();
                    } else {
                        browser.read(cx).find(true);
                    }
                } else if let Some(editor) = &view.read(cx).source_editor {
                    let find = editor.find.clone();
                    find.update(cx, |find, cx| {
                        if toggle {
                            find.toggle(window, cx);
                        } else {
                            find.show(replace, window, cx);
                        }
                    });
                }
                return;
            }
            _ => {}
        }
        let find = match &self.renderer {
            Renderer::Markup(m) if m.preview && !m.html => &m.find,
            _ => &self.source().expect("searchable source").find,
        };
        find.update(cx, |find, cx| {
            if toggle {
                find.toggle(window, cx);
            } else {
                find.show(replace, window, cx);
            }
        });
    }
    pub fn preview_text_state(&self) -> Option<Entity<gpui_kit::base::TextViewState>> {
        match &self.renderer {
            Renderer::Markup(markup) if markup.preview && !markup.html => {
                Some(markup.state.clone())
            }
            _ => None,
        }
    }
    pub(crate) fn refresh_dependencies(&mut self, cx: &mut App) {
        if let Renderer::Markup(markup) = &mut self.renderer {
            markup.refresh_dependencies(cx);
        }
    }
    pub fn set_task_toggle_handler(&mut self, handler: ToggleTask, cx: &App) {
        let editable = self
            .editor()
            .is_some_and(|editor| editor.read(cx).context_menu_capabilities().is_editable());
        if let Renderer::Markup(markup) = &mut self.renderer {
            markup.task_toggle = editable.then_some(handler);
        }
    }
    pub fn toggle_task(
        &mut self,
        range: std::ops::Range<usize>,
        checked: bool,
        revision: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if let Renderer::Markup(markup) = &mut self.renderer {
            markup.toggle_task(range, checked, revision, window, cx)
        } else {
            false
        }
    }
    pub(super) fn source(&self) -> Option<&SourceReader> {
        match &self.renderer {
            Renderer::Source(s) => Some(s),
            Renderer::Markup(m) => Some(&m.source),
            _ => None,
        }
    }
    pub fn inherit_view(&mut self, old: &Self, cx: &mut App) {
        self.document.inherit_identity(&old.document);
        match (&mut self.renderer, &old.renderer) {
            (Renderer::Source(s), Renderer::Source(o)) => s.inherit(o, cx),
            (Renderer::Markup(s), Renderer::Markup(o)) => {
                s.source.inherit(&o.source, cx);
                s.inherit_preview_scroll(o, cx);
            }
            _ => {}
        }
        if old.can_preview() && self.can_preview() {
            self.set_preview(old.mode() == ReaderMode::Preview, cx);
        }
    }
    pub fn can_word_wrap(&self) -> bool {
        self.source().is_some()
    }
    pub fn set_word_wrap(&self, enabled: bool, window: &mut Window, cx: &mut App) {
        if let Some(source) = self.source() {
            source
                .edit
                .editor()
                .update(cx, |s, cx| s.set_soft_wrap(enabled, window, cx));
        }
    }
    pub fn horizontal_offsets(&self, cx: &App) -> Option<Vec<Pixels>> {
        if self.projection.as_ref().is_some_and(Projection::visible) {
            return Some(Vec::new());
        }
        match &self.renderer {
            Renderer::Paged { .. } => None,
            Renderer::Markup(m) if m.preview && m.html => None,
            Renderer::Markup(m) if m.preview => Some(vec![m.scroll.offset().x]),
            _ => Some(
                self.source()
                    .map(|s| vec![s.edit.editor().read(cx).scroll_offset().x])
                    .unwrap_or_default(),
            ),
        }
    }
    pub fn focus_source(&self, window: &mut Window, cx: &mut App) {
        if let Some(source) = self.source() {
            source.edit.editor().update(cx, |s, cx| s.focus(window, cx));
        }
    }
    pub fn table_format(&self) -> Option<TableFormat> {
        self.projection.as_ref().and_then(Projection::table_format)
    }
    pub fn is_csv(&self) -> bool {
        self.table_format().is_some_and(TableFormat::is_delimited)
    }
    pub(super) fn geo_format(&self) -> Option<&'static str> {
        self.table_format().and_then(TableFormat::geo_name)
    }
    pub fn is_json(&self) -> bool {
        self.projection.as_ref().is_some_and(Projection::is_json)
    }
    pub fn is_openapi(&self) -> bool {
        self.projection.as_ref().is_some_and(Projection::is_openapi)
    }
    pub(super) fn restore_csv_preview(&mut self, cx: &mut App) {
        if let Some(kind) = self.table_format().and_then(|_| self.preview_preference())
            && preview_enabled(kind, cx)
        {
            self.set_preview(true, cx);
        }
    }
    pub fn preview_preference(&self) -> Option<PreviewPreference> {
        if matches!(&self.renderer, Renderer::Paged { html: Some(_), .. }) {
            return Some(PreviewPreference::Html);
        }
        if self.is_csv() {
            return Some(PreviewPreference::Csv);
        }
        if self.geo_format().is_some() {
            return Some(PreviewPreference::Markdown);
        }
        match &self.renderer {
            Renderer::Markup(m) => Some(if m.html {
                PreviewPreference::Html
            } else {
                PreviewPreference::Markdown
            }),
            _ => None,
        }
    }
    pub fn can_preview(&self) -> bool {
        self.projection.is_some()
            || matches!(
                self.renderer,
                Renderer::Markup(_) | Renderer::Paged { html: Some(_), .. }
            )
    }
    pub fn set_preview(&mut self, preview: bool, cx: &mut App) {
        if !preview && glim_services::diagnostics::failure(&self.document.path).is_some() {
            glim_services::diagnostics::retry(&self.document.path);
        }
        if preview
            && self.is_dirty()
            && let Renderer::Paged { view, .. } = &self.renderer
        {
            view.update(cx, |page, cx| page.request_save_before_preview(cx));
            return;
        }
        if let Renderer::Paged {
            html: Some(html), ..
        } = &mut self.renderer
        {
            let mode = if preview {
                ReaderMode::Preview
            } else {
                ReaderMode::Source
            };
            html.set_mode(mode, cx);
        }
        let text = if preview {
            self.projection.as_ref().and_then(|projection| {
                self.source().map(|source| {
                    projection.snapshot(source.edit.editor().read(cx).text().chunks(), cx)
                })
            })
        } else {
            None
        };
        if let Some(projection) = &mut self.projection {
            projection.set_visible(preview, text, cx);
        }
        if let Renderer::Markup(m) = &mut self.renderer {
            if preview {
                m.sync_preview(cx);
            }
            m.preview = preview;
            if !preview {
                m.set_images_active(false, cx);
            }
        }
    }
    pub fn mode(&self) -> ReaderMode {
        if self.projection.as_ref().is_some_and(Projection::visible)
            || matches!(
                &self.renderer,
                Renderer::Paged { html: Some(html), .. } if html.preview().is_some()
            )
            || matches!(&self.renderer, Renderer::Markup(m) if m.preview)
        {
            ReaderMode::Preview
        } else {
            ReaderMode::Source
        }
    }

    pub(crate) fn can_export_markdown(&self) -> bool {
        matches!(&self.renderer, Renderer::Markup(markup) if markup.preview && !markup.html)
            && self.toolbar_profile() == "markdown"
    }

    pub(crate) fn markdown_export_snapshot(
        &self,
        cx: &App,
    ) -> Option<(
        gpui_kit::component::input::Rope,
        url::Url,
        Option<std::path::PathBuf>,
    )> {
        let Renderer::Markup(markup) = &self.renderer else {
            return None;
        };
        self.can_export_markdown().then(|| {
            (
                markup.source.edit.editor().read(cx).text().clone(),
                markup.base.clone(),
                markup.format_path.clone(),
            )
        })
    }
    pub fn reveal_content_offset(&mut self, offset: u64, window: &mut Window, cx: &mut App) {
        self.set_preview(false, cx);
        if let Renderer::Paged { view, .. } = &self.renderer {
            view.update(cx, |view, cx| view.reveal_offset(offset, window, cx));
        } else if let Some(source) = self.source() {
            source.edit.editor().update(cx, |editor, cx| {
                let offset = usize::try_from(offset).unwrap_or(usize::MAX);
                editor.set_selected_range(offset..offset, cx);
                editor.focus(window, cx);
            });
        }
    }
    pub fn reveal_anchor(&mut self, anchor: &str, cx: &mut App) {
        if let Renderer::Paged {
            html: Some(html), ..
        } = &self.renderer
        {
            html.reveal_anchor(anchor, cx);
            self.set_preview(true, cx);
            return;
        }
        if let Renderer::Markup(m) = &mut self.renderer {
            m.sync_preview(cx);
            m.reveal(anchor, cx);
        } else if let Some(source) = self.source()
            && let Some(line) = anchor
                .strip_prefix('L')
                .and_then(|s| s.split('-').next())
                .and_then(|s| s.parse::<usize>().ok())
        {
            source.edit.editor().update(cx, |s, cx| {
                let height = s.line_height().unwrap_or(px(20.));
                s.set_scroll_offset(point(px(0.), -height * line.saturating_sub(1) as f32), cx);
            });
        }
    }
}
