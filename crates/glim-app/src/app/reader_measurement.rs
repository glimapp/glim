//! Opt-in real-window measurements. No workspace, persistence or telemetry startup.
use crate::views::Reader;
use anyhow::{Context as _, Result, bail, ensure};
use gpui_kit::{
    component::{ActiveTheme, Theme, ThemeMode, input::EditorState},
    profiler::{FrameEvent, FrameTimingCollector},
    *,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

struct Driver(Option<Task<()>>);
impl Global for Driver {}

#[derive(Clone, Copy)]
struct Paint {
    at: Instant,
    bytes: usize,
    scroll: Point<Pixels>,
    rows: (usize, usize),
}
type Paints = Rc<RefCell<Vec<Paint>>>;

struct MeasurementView {
    reader: Option<Reader>,
    paints: Paints,
}
impl Render for MeasurementView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = self.reader.as_mut().map(|reader| {
            let editor = reader.editor().expect("source reader validated at load");
            Probe {
                content: reader.render(Arc::new(|_, _, _, _| {}), cx),
                editor,
                paints: self.paints.clone(),
            }
        });
        div()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .children(content)
    }
}

// Observe after the production subtree has painted, not after reader construction.
struct Probe {
    content: AnyElement,
    editor: Entity<EditorState>,
    paints: Paints,
}
impl IntoElement for Probe {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Probe {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.content
            .prepaint_as_root(bounds.origin, bounds.size.into(), window, cx);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.content.paint(window, cx);
        let editor = self.editor.read(cx);
        if let Some(rows) = editor.visible_row_range()
            && !rows.is_empty()
            && editor
                .text_bounds()
                .is_some_and(|bounds| bounds.size.width > px(0.) && bounds.size.height > px(0.))
        {
            self.paints.borrow_mut().push(Paint {
                at: Instant::now(),
                bytes: editor.text().len(),
                scroll: editor.scroll_offset(),
                rows: (rows.start, rows.end),
            });
        }
    }
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.
}

fn print_outcome(result: &Result<Value>) {
    // NSApplication termination can exit without returning from Application::run.
    let value = match result {
        Ok(value) => value.clone(),
        Err(error) => json!({ "error": format!("{error:#}") }),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&value).expect("JSON values serialize")
    );
}

async fn presented(
    cx: &AsyncApp,
    collector: &mut FrameTimingCollector,
    paints: &Paints,
    window: WindowId,
    started: Instant,
    expected_bytes: usize,
) -> Result<(Value, Paint)> {
    let mut drawn = None;
    loop {
        for event in collector.collect_unseen() {
            match event {
                FrameEvent::Draw(frame) if frame.window_id == window => {
                    drawn = paints
                        .borrow()
                        .iter()
                        .copied()
                        .find(|paint| {
                            paint.at >= started
                                && paint.at >= frame.draw_start
                                && paint.at <= frame.draw_end
                                && paint.bytes == expected_bytes
                        })
                        .map(|paint| (frame, paint));
                }
                FrameEvent::Present(frame) if frame.window_id == window => {
                    if let Some((draw, paint)) = drawn.take() {
                        paints.borrow_mut().clear();
                        return Ok((
                            json!({
                                "to_content_paint_ms": ms(paint.at.duration_since(started)),
                                "to_frame_submission_ms": ms(frame.present_end.duration_since(started)),
                                "draw_ms": ms(draw.draw_duration()),
                                "submission_ms": ms(frame.present_duration()),
                                "visible_rows": [paint.rows.0, paint.rows.1],
                            }),
                            paint,
                        ));
                    }
                }
                _ => {}
            }
        }
        ensure!(
            started.elapsed() < Duration::from_secs(30),
            "no matching readable frame within 30 seconds"
        );
        cx.background_executor()
            .timer(Duration::from_millis(4))
            .await;
    }
}

pub(crate) fn run() -> Result<()> {
    let mut arguments = std::env::args_os().skip(2);
    let path = PathBuf::from(
        arguments
            .next()
            .context("Usage: glim --measure-source-reader fixture.txt")?,
    )
    .canonicalize()?;
    ensure!(arguments.next().is_none(), "expected one fixture path");
    ensure!(
        path.extension().is_some_and(|extension| extension == "txt"),
        "source measurements require a .txt fixture"
    );
    let outcome = Rc::new(RefCell::new(None));
    let result = outcome.clone();
    gpui_kit::application()
        .with_assets(super::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            Theme::change(ThemeMode::Light, None, cx);
            cx.set_quit_mode(QuitMode::Explicit);
            cx.set_global(Driver(None));
            profiler::set_trace_enabled(true);
            let paints = Paints::default();
            let created = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1000.), px(720.)),
                        cx,
                    ))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Glim source measurement".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                |_, cx| {
                    cx.new(|_| MeasurementView {
                        reader: None,
                        paints: paints.clone(),
                    })
                },
            );
            let (handle, view) = match created {
                Ok(created) => created,
                Err(error) => {
                    let error = Err(error);
                    print_outcome(&error);
                    *result.borrow_mut() = Some(error);
                    cx.quit();
                    return;
                }
            };
            cx.activate(true);
            let task = cx.spawn(async move |cx| {
                let measured = measure(path, handle, view, paints, cx).await;
                print_outcome(&measured);
                *result.borrow_mut() = Some(measured);
                cx.update(|cx| cx.quit());
            });
            cx.global_mut::<Driver>().0 = Some(task);
        });
    outcome
        .borrow_mut()
        .take()
        .context("measurement window closed before completion")??;
    Ok(())
}

async fn measure(
    path: PathBuf,
    handle: AnyWindowHandle,
    view: Entity<MeasurementView>,
    paints: Paints,
    cx: &mut AsyncApp,
) -> Result<Value> {
    // Exclude window/platform initialization; include document queueing, reading and construction.
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    let mut collector = FrameTimingCollector::new();
    let opened = Instant::now();
    let input_path = path.clone();
    let content = super::reader_jobs::run(
        cx.background_executor().clone(),
        super::reader_jobs::JobClass::Document,
        super::reader_jobs::Priority::Interactive,
        Default::default(),
        move |cancel| glim_services::documents::read_content_cancellable(&input_path, cancel),
    )
    .await?;
    let read_ms = ms(opened.elapsed());
    let glim_services::documents::Content::File(document) = &content else {
        bail!("fixture selected a paged or non-source reader; no source result reported");
    };
    ensure!(!document.text.is_empty(), "fixture must contain text");
    let copied = Instant::now();
    let original = document.text.clone();
    let validation_copy_ms = ms(copied.elapsed());
    let constructed = Instant::now();
    let (editor, window_id, scale) = handle.update(cx, |_, window, cx| {
        let mut reader = Reader::from_content(content, window, cx);
        reader.set_preview(false, cx);
        reader.activate(cx);
        reader.focus_source(window, cx);
        let editor = reader.editor().context("fixture has no source editor")?;
        view.update(cx, |view, cx| {
            view.reader = Some(reader);
            cx.notify();
        });
        Ok::<_, anyhow::Error>((
            editor,
            window.window_handle().window_id(),
            window.scale_factor(),
        ))
    })??;
    let construction_ms = ms(constructed.elapsed());
    let (first, _) = presented(
        cx,
        &mut collector,
        &paints,
        window_id,
        opened,
        original.len(),
    )
    .await?;
    let mut inputs = Vec::new();
    let middle = original.floor_char_boundary(original.len() / 2);
    let positions = [0, middle, original.len()];
    for index in 0..48 {
        // Alternating insertion/backspace exercises both dirty and saved-equivalent states.
        let insert = index % 2 == 0;
        let offset = positions[(index / 2) % positions.len()];
        if insert {
            let moved = Instant::now();
            editor.update(cx, |editor, cx| {
                editor.set_selected_range(offset..offset, cx)
            });
            presented(
                cx,
                &mut collector,
                &paints,
                window_id,
                moved,
                original.len(),
            )
            .await?;
        }
        collector.collect_unseen();
        paints.borrow_mut().clear();
        let started = Instant::now();
        let dispatched = handle.update(cx, |_, window, cx| {
            window.dispatch_keystroke(
                Keystroke::parse(if insert { "x" } else { "backspace" }).expect("fixed keystroke"),
                cx,
            )
        })?;
        let dispatch_ms = ms(started.elapsed());
        ensure!(dispatched, "editor did not accept scripted input");
        let expected = original.len() + usize::from(insert);
        let (mut sample, _) =
            presented(cx, &mut collector, &paints, window_id, started, expected).await?;
        view.update(cx, |view, cx| {
            let text = editor.read(cx).text();
            let actual = text.to_string();
            ensure!(
                actual
                    == if insert {
                        format!("{}x{}", &original[..offset], &original[offset..])
                    } else {
                        original.clone()
                    },
                "scripted input produced unexpected content"
            );
            ensure!(
                view.reader.as_ref().unwrap().is_dirty() == insert,
                "dirty state disagrees with saved content"
            );
            Ok::<_, anyhow::Error>(())
        })?;
        sample["dispatch_ms"] = json!(dispatch_ms);
        sample["source_offset"] = json!(offset);
        sample["operation"] = json!(if insert { "insert" } else { "backspace" });
        inputs.push(sample);
    }
    let reset = Instant::now();
    editor.update(cx, |editor, cx| {
        editor.set_selected_range(0..0, cx);
        editor.set_scroll_offset(Point::default(), cx);
    });
    presented(
        cx,
        &mut collector,
        &paints,
        window_id,
        reset,
        original.len(),
    )
    .await?;
    let mut scrolls = Vec::new();
    for index in 0..24 {
        collector.collect_unseen();
        paints.borrow_mut().clear();
        let before = editor.read_with(cx, |editor, _| editor.scroll_offset());
        let started = Instant::now();
        handle.update(cx, |_, window, cx| {
            window.dispatch_event(
                PlatformInput::ScrollWheel(ScrollWheelEvent {
                    position: point(px(400.), px(300.)),
                    delta: ScrollDelta::Pixels(point(
                        px(0.),
                        if index < 12 { px(-80.) } else { px(80.) },
                    )),
                    touch_phase: TouchPhase::Moved,
                    ..Default::default()
                }),
                cx,
            );
        })?;
        let dispatch_ms = ms(started.elapsed());
        let (mut sample, paint) = presented(
            cx,
            &mut collector,
            &paints,
            window_id,
            started,
            original.len(),
        )
        .await?;
        ensure!(
            paint.scroll != before,
            "scroll did not move the viewport; use a longer fixture"
        );
        sample["dispatch_ms"] = json!(dispatch_ms);
        sample["scroll_y"] = json!(f32::from(paint.scroll.y));
        scrolls.push(sample);
    }
    Ok(json!({
        "schema": 1, "scenario": "plain-source", "fixture": path,
        "bytes": original.len(), "window_logical_pixels": [1000, 720], "scale_factor": scale,
        "open": { "queue_and_read_ms": read_ms, "construction_ms": construction_ms, "validation_copy_ms": validation_copy_ms, "first_readable_viewport": first },
        "input": inputs, "scroll": scrolls,
        "scope": "existing window; production document job, Reader, editor and minimap; scripted GPUI events to matching content paint and platform frame submission",
        "excludes": ["workspace restore", "autosave", "hardware input", "GPU completion and display scanout", "native and preview readers", "cache-hit measurement", "process peak memory"],
    }))
}
