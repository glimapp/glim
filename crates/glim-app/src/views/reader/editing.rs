use super::*;

impl Reader {
    pub fn untitled(directory: PathBuf, window: &mut Window, cx: &mut App) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!("{}-{number}", glim_services::locale::tr("Untitled"));
        let mut reader = Self::new(
            Document {
                path: PathBuf::from("/__glim_untitled__").join(&name),
                kind: DocumentKind::Text,
                text: String::new(),
            },
            window,
            cx,
        );
        reader.title = name.into();
        reader.document.draft_directory = Some(directory);
        reader.document.recovery_key = Some(format!(
            "{}-{}-{number}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            std::process::id()
        ));
        reader
    }
    pub fn restore_draft_text(&mut self, text: String, window: &mut Window, cx: &mut App) {
        if let Renderer::Source(source) = &mut self.renderer {
            source
                .edit
                .editor()
                .update(cx, |editor, cx| editor.set_value(text.clone(), window, cx));
            source.mark_saved(String::new(), cx);
            let text = source.edit.editor().read(cx).text().clone();
            source.minimap.update(cx, |map, cx| map.set_text(&text, cx));
        }
    }
    pub fn bind_saved_draft(
        &mut self,
        path: PathBuf,
        saved: String,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Keep the live editor, selection and undo history when adopting a path.
        let current = self.editor().unwrap().read(cx).value().to_string();
        let mut reader = Self::new(
            Document {
                kind: DocumentKind::from_path(&path),
                path,
                text: current,
            },
            window,
            cx,
        );
        std::mem::swap(&mut self.renderer, &mut reader.renderer);
        if let Renderer::Source(source) = reader.renderer {
            match &mut self.renderer {
                Renderer::Source(target) => *target = source,
                Renderer::Markup(target) => target.source = source,
                _ => unreachable!("text document has a source editor"),
            }
        }
        self.document.path = reader.document.path;
        self.title = reader.title;
        self.projection = reader.projection;
        self.structured_controls = TextPresentation::for_path(&self.document.path)
            .structured
            .then(|| {
                cx.new(|_| json_format::JsonFormat::new(self.document.path.clone(), self.editor()))
            });
        self.document.draft_directory = None;
        if let Some(editor) = self.editor() {
            let language = language_for_path(&self.document.path);
            editor.update(cx, |editor, cx| {
                if lexical::supports(language) {
                    editor.set_highlighter_factory(
                        std::rc::Rc::new(|name| Some(Box::new(lexical::Lexical::new(name)))),
                        cx,
                    );
                }
                editor.set_highlighter(language, cx);
            });
        }
        self.set_preview(false, cx);
        self.mark_saved(saved, cx);
    }
    pub fn editor(&self) -> Option<Entity<gpui_kit::component::input::EditorState>> {
        self.document
            .edit
            .as_ref()
            .map(|edit| edit.editor().clone())
    }
    pub fn set_editing_locked(&mut self, locked: bool, cx: &mut App) {
        if let Some(edit) = &self.document.edit {
            edit.set_locked(locked || self.document.remote.is_some(), cx);
        }
    }
    pub(crate) fn set_remote(
        &mut self,
        session: glim_services::remote::Session,
        path: std::path::PathBuf,
        cx: &mut App,
    ) {
        self.document.path = path;
        self.document.remote = Some(session);
        self.set_editing_locked(true, cx);
    }
    pub fn is_dirty(&self) -> bool {
        self.document
            .edit
            .as_ref()
            .is_some_and(|edit| edit.is_dirty())
    }
    pub fn page_save_request(&self, cx: &mut App) -> Option<glim_services::paged::PageEdit> {
        if let Renderer::Paged { view, .. } = &self.renderer {
            view.update(cx, |page, _| page.save_request())
        } else {
            None
        }
    }
    pub fn page_saved(
        &mut self,
        text: String,
        saved: glim_services::paged::PageSaved,
        cx: &mut App,
    ) {
        if let Renderer::Paged { view, .. } = &self.renderer {
            self.document
                .mark_saved(ContentSnapshot::Fingerprint(format!(
                    "paged:{}:{:?}",
                    saved.total, saved.modified
                )));
            view.update(cx, |page, cx| page.saved(text, saved, cx));
            if let Renderer::Paged {
                html: Some((browser, _)),
                ..
            } = &self.renderer
            {
                browser.update(cx, |browser, cx| browser.reload_html_file(cx));
            }
        }
    }
    pub fn finish_page_save(&self) {
        if let Renderer::Paged { saving, .. } = &self.renderer {
            saving.set(false);
        }
    }
    pub fn mark_saved(&mut self, text: String, cx: &App) {
        self.document
            .mark_saved(ContentSnapshot::Text(text.clone().into()));
        if let Some(edit) = &self.document.edit {
            edit.mark_saved(text, cx);
        }
    }
}

#[cfg(test)]
mod editing_tests {
    use super::SourceReader;
    use super::{Reader, Renderer};
    use glim_core::{Document, DocumentKind};
    use gpui_kit::ScrollHandle;
    use gpui_kit::{
        App, AppContext, Bounds, Context, Entity, IntoElement, Point, Render, TestAppContext,
        Window, WindowBounds, WindowOptions, px, size, test::TestWindowExt,
    };

    struct Fixture(Reader);
    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.0.render(std::sync::Arc::new(|_, _, _, _| {}), cx)
        }
    }
    fn open(
        cx: &mut TestAppContext,
        build: impl FnOnce(&mut Window, &mut App) -> Reader,
    ) -> (gpui_kit::AnyWindowHandle, Entity<Fixture>) {
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point::default(),
                        size: size(px(900.), px(600.)),
                    })),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| Fixture(build(window, cx))),
            )
            .unwrap()
        })
    }
    #[gpui_kit::test]
    fn edit_tracking_outlives_its_source_view(cx: &mut TestAppContext) {
        let (handle, _) = open(cx, |window, cx| {
            Reader::untitled(std::env::temp_dir(), window, cx)
        });
        let edit = cx
            .update_window(handle, |_, window, cx| {
                let source =
                    SourceReader::new("saved", "plain", false, ScrollHandle::new(), window, cx);
                source.edit.clone()
            })
            .unwrap();
        let before = edit.revision();
        cx.update_window(handle, |_, window, cx| {
            edit.editor().update(cx, |editor, cx| {
                editor.replace_all("newer input", window, cx)
            });
        })
        .unwrap();
        cx.run_until_parked();
        assert!(edit.is_dirty());
        assert_ne!(edit.revision(), before);
        cx.update(|cx| {
            edit.mark_saved("saved", cx);
            assert!(edit.is_dirty());
            edit.mark_saved("newer input", cx);
            assert!(!edit.is_dirty());
            edit.set_locked(true, cx);
            assert!(
                !edit
                    .editor()
                    .read(cx)
                    .context_menu_capabilities()
                    .is_editable()
            );
        });
    }

    #[gpui_kit::test]
    fn first_save_retains_live_edits_and_recovery_until_latest_contents_are_saved(
        cx: &mut TestAppContext,
    ) {
        let (handle, view) = open(cx, |window, cx| {
            Reader::untitled(std::env::temp_dir(), window, cx)
        });
        let editor = cx.update(|cx| view.read(cx).0.editor().unwrap());
        cx.update_window(handle, |_, window, cx| {
            editor.update(cx, |editor, cx| {
                editor.replace_all("saved snapshot\nnewer input", window, cx)
            });
            view.update(cx, |view, cx| {
                view.0.bind_saved_draft(
                    std::env::temp_dir().join("first-save.txt"),
                    "saved snapshot".into(),
                    window,
                    cx,
                )
            });
        })
        .unwrap();
        cx.update(|cx| {
            let reader = &view.read(cx).0;
            assert_eq!(reader.editor().unwrap().entity_id(), editor.entity_id());
            assert_eq!(
                editor.read(cx).value().as_ref(),
                "saved snapshot\nnewer input"
            );
            assert!(reader.is_dirty());
            assert!(reader.document.draft_directory.is_none());
            assert!(reader.document.recovery_key.is_some());
            assert_eq!(
                reader.document.snapshot.saved_text().as_deref(),
                Some("saved snapshot")
            );
        });
    }
    #[gpui_kit::test]
    fn recovered_notes_are_dirty_and_keep_their_original_content(cx: &mut TestAppContext) {
        let (_, view) = open(cx, |window, cx| {
            let mut reader = Reader::untitled(std::env::temp_dir(), window, cx);
            reader.restore_draft_text("Recovered 中文\r\ntext".into(), window, cx);
            reader
        });
        cx.update(|cx| {
            let reader = &view.read(cx).0;
            assert!(reader.is_dirty());
            assert_eq!(
                reader.editor().unwrap().read(cx).value().as_ref(),
                "Recovered 中文\r\ntext"
            );
        });
    }
    #[gpui_kit::test]
    fn source_right_click_does_not_reborrow_editor(cx: &mut TestAppContext) {
        for (name, kind) in [
            ("test.rs", DocumentKind::Text),
            ("test.md", DocumentKind::Markdown),
            ("test.html", DocumentKind::Text),
        ] {
            let (handle, view) = open(cx, |window, cx| {
                let mut reader = Reader::new(
                    Document {
                        path: std::env::temp_dir().join(name),
                        kind,
                        text: "Sample text\n".into(),
                    },
                    window,
                    cx,
                );
                reader.set_preview(false, cx);
                reader
            });
            let editor = cx.update(|cx| view.read(cx).0.editor().unwrap());
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.right_click(("input", editor.entity_id()), cx);
            })
            .unwrap();
            cx.run_until_parked();
        }
    }

    #[gpui_kit::test]
    fn markdown_preview_uses_unsaved_edits(cx: &mut TestAppContext) {
        let (handle, view) = open(cx, |window, cx| {
            let mut reader = Reader::new(
                Document {
                    path: std::env::temp_dir().join("test-note.md"),
                    kind: DocumentKind::Markdown,
                    text: "# Original\n".into(),
                },
                window,
                cx,
            );
            reader.set_preview(false, cx);
            reader
        });
        let editor = cx.update(|cx| view.read(cx).0.editor().unwrap());
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click(("input", editor.entity_id()), cx);
            window.press("secondary-a", cx);
            window.input("# Edited heading\n", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert!(cx.update(|cx| view.read(cx).0.is_dirty()));
        cx.update(|cx| view.update(cx, |v, cx| v.0.set_preview(true, cx)));
        cx.run_until_parked();
        cx.update(|cx| {
            let Renderer::Markup(markdown) = &view.read(cx).0.renderer else {
                unreachable!()
            };
            assert_eq!(
                markdown.source.edit.editor().read(cx).value().as_ref(),
                "# Edited heading\n"
            );
            assert!(
                markdown
                    .state
                    .read(cx)
                    .rendered_text()
                    .as_str()
                    .contains("Edited heading")
            );
            assert_eq!(
                view.read(cx).0.document.snapshot.saved_text().as_deref(),
                Some("# Original\n")
            );
        });
    }
    #[gpui_kit::test]
    fn byte_preview_rejects_typing_and_has_no_save_buffer(cx: &mut TestAppContext) {
        let (handle, view) = open(cx, |window, cx| {
            Reader::from_bytes(
                glim_services::binary::BytePreview {
                    path: std::env::temp_dir().join(".DS_Store"),
                    text: "00000000  00 ff 41".into(),
                },
                window,
                cx,
            )
        });
        let editor = cx.update(|cx| view.read(cx).0.source().unwrap().edit.editor().clone());
        let original = cx.update(|cx| editor.read(cx).value());
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click(("input", editor.entity_id()), cx);
            window.press("secondary-a", cx);
            window.input("must not overwrite binary bytes", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(editor.read(cx).value(), original);
            assert!(view.read(cx).0.editor().is_none());
            assert!(!view.read(cx).0.is_dirty());
        });
    }
}
