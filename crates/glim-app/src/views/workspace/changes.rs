use glim_services::{
    git::{self, Area, Change, Repository},
    locale::tr,
    work::{CancelOnDrop, Cancellation},
};
use gpui_kit::{
    component::{
        ActiveTheme, Disableable, Selectable, Sizable,
        button::{Button, ButtonVariants},
        diff::{Diff, DiffFile, DiffMode, DiffState},
        h_flex, v_flex,
    },
    prelude::FluentBuilder,
    *,
};
use std::{path::PathBuf, sync::Arc};

type NavigationRow = (PathBuf, Option<Change>);

pub(super) struct OpenFile(pub PathBuf);
impl EventEmitter<OpenFile> for Changes {}
pub(super) struct ShowDiff;
impl EventEmitter<ShowDiff> for Changes {}

enum Prepared {
    Diff(Vec<DiffFile>, glim_services::resources::WorkingReservation),
    Raw(glim_services::paged::TextPage),
}

pub(super) struct Changes {
    roots: Vec<PathBuf>,
    remote: Option<glim_services::remote::Session>,
    repositories: Vec<Repository>,
    rows: Arc<Vec<NavigationRow>>,
    scroll: UniformListScrollHandle,
    errors: Vec<SharedString>,
    loading: bool,
    selection: Option<(PathBuf, Change)>,
    diff: Entity<DiffState>,
    memory: Option<glim_services::resources::WorkingReservation>,
    raw: Option<Entity<crate::views::reader::paged::PagedReader>>,
    patch_loading: bool,
    patch_error: Option<SharedString>,
    soft_wrap: bool,
    generation: u64,
    status_task: Option<Task<()>>,
    status_cancel: Option<CancelOnDrop>,
    patch_task: Option<Task<()>>,
    patch_cancel: Option<CancelOnDrop>,
}

impl Changes {
    pub(super) fn cancel(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        self.status_cancel = None;
        self.patch_cancel = None;
        self.status_task = None;
        self.patch_task = None;
        self.loading = false;
        self.patch_loading = false;
        self.clear_patch(cx);
    }

    pub(super) fn unavailable(&mut self, cx: &mut Context<Self>) {
        self.cancel(cx);
        self.repositories.clear();
        self.rows = Arc::default();
        self.selection = None;
        self.clear_patch(cx);
        self.errors = vec![tr("Connect to the remote workspace to view changes").into()];
        cx.notify();
    }
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            roots: Vec::new(),
            remote: None,
            repositories: Vec::new(),
            rows: Arc::default(),
            scroll: UniformListScrollHandle::new(),
            errors: Vec::new(),
            loading: false,
            selection: None,
            diff: cx.new(|cx| DiffState::new([], cx)),
            memory: None,
            raw: None,
            patch_loading: false,
            patch_error: None,
            soft_wrap: false,
            generation: 0,
            status_task: None,
            status_cancel: None,
            patch_task: None,
            patch_cancel: None,
        }
    }

    pub(super) fn load(
        &mut self,
        roots: Vec<PathBuf>,
        remote: Option<glim_services::remote::Session>,
        cx: &mut Context<Self>,
    ) {
        self.status_cancel = None;
        self.status_task = None;
        self.patch_cancel = None;
        self.patch_task = None;
        self.patch_loading = false;
        self.generation += 1;
        let generation = self.generation;
        if self.roots != roots
            || self.remote.as_ref().map(|r| &r.connection.id)
                != remote.as_ref().map(|r| &r.connection.id)
        {
            self.selection = None;
            self.repositories.clear();
            self.rows = Arc::default();
            self.clear_patch(cx);
        }
        self.roots = roots.clone();
        self.remote = remote.clone();
        self.loading = true;
        self.errors.clear();
        let (cancel, guard) = Cancellation::task();
        self.status_cancel = Some(guard);
        let work = cx.background_executor().spawn(async move {
            roots
                .into_iter()
                .map(|root| {
                    git::changes(&root, remote.as_ref(), &cancel)
                        .map_err(|error| format!("{}: {error:#}", root.display()))
                })
                .collect::<Vec<_>>()
        });
        self.status_task = Some(cx.spawn(async move |view, cx| {
            let results = work.await;
            let _ = view.update(cx, |view, cx| {
                if view.generation != generation {
                    return;
                }
                view.loading = false;
                view.repositories.clear();
                for result in results {
                    match result {
                        Ok(repository) => view.repositories.push(repository),
                        Err(error) => view.errors.push(error.into()),
                    }
                }
                let mut rows = Vec::new();
                for repository in &view.repositories {
                    rows.push((repository.root.clone(), None));
                    for area in [Area::Staged, Area::Working, Area::Untracked] {
                        rows.extend(
                            repository
                                .changes
                                .iter()
                                .filter(|change| change.area == area)
                                .map(|change| (repository.root.clone(), Some(change.clone()))),
                        );
                    }
                }
                view.rows = Arc::new(rows);
                let selection = view
                    .selection
                    .as_ref()
                    .filter(|(root, selected)| {
                        view.repositories.iter().any(|repository| {
                            &repository.root == root && repository.changes.contains(selected)
                        })
                    })
                    .cloned()
                    .or_else(|| {
                        view.repositories.iter().find_map(|repository| {
                            repository
                                .changes
                                .first()
                                .map(|change| (repository.root.clone(), change.clone()))
                        })
                    });
                if let Some((root, change)) = selection {
                    view.select(root, change, cx);
                } else {
                    view.selection = None;
                    view.clear_patch(cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn clear_patch(&mut self, cx: &mut Context<Self>) {
        self.diff.update(cx, |state, cx| state.set_files([], cx));
        self.raw = None;
        self.memory = None;
        self.patch_error = None;
    }

    fn select(&mut self, root: PathBuf, change: Change, cx: &mut Context<Self>) {
        self.patch_cancel = None;
        self.patch_task = None;
        self.clear_patch(cx);
        self.selection = Some((root.clone(), change.clone()));
        self.patch_loading = true;
        let generation = self.generation;
        let selection = self.selection.clone();
        let remote = self.remote.clone();
        let (cancel, guard) = Cancellation::task();
        self.patch_cancel = Some(guard);
        let work = cx.background_executor().spawn(async move {
            let patch = git::patch(&root, &change, remote.as_ref(), &cancel)?;
            let bytes = patch.as_file().metadata()?.len();
            let mut input = patch.reopen()?;
            let mut buffer = vec![0; glim_services::limits::IO_BUFFER_BYTES];
            let mut lines = 0u64;
            loop {
                use std::io::Read as _;
                cancel.check()?;
                let read = input.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                lines = lines.saturating_add(
                    buffer[..read].iter().filter(|byte| **byte == b'\n').count() as u64,
                );
            }
            // Reserve a conservative parser envelope before the library allocates
            // per-line strings, source ranges and presentation rows.
            let envelope = bytes
                .saturating_mul(std::mem::size_of::<usize>() as u64)
                .saturating_add(lines.saturating_mul(std::mem::size_of::<DiffFile>() as u64));
            let memory = glim_services::resources::pool().reserve_working(envelope, envelope);
            let mut binary = false;
            if let Ok(memory) = memory {
                let text = glim_services::resources::read_to_end(patch.reopen()?, &cancel);
                match text {
                    Ok(text) => match String::from_utf8(text) {
                        Ok(text) => {
                            cancel.check()?;
                            let files = DiffFile::parse(&text)?;
                            cancel.check()?;
                            return Ok::<_, anyhow::Error>(Prepared::Diff(files, memory));
                        }
                        Err(_) => binary = true,
                    },
                    Err(error) if !glim_services::resources::is_memory_pressure(&error) => {
                        return Err(error);
                    }
                    Err(_) => (),
                }
            }
            cancel.check()?;
            let path = change.path.with_extension("diff");
            Ok(Prepared::Raw(if binary {
                glim_services::paged::generated_binary(&path, patch)?
            } else {
                glim_services::paged::generated(&path, patch)?
            }))
        });
        self.patch_task = Some(cx.spawn(async move |view, cx| {
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                if view.generation != generation || view.selection != selection {
                    return;
                }
                view.patch_loading = false;
                match result {
                    Ok(Prepared::Diff(files, memory)) => {
                        view.diff.update(cx, |state, cx| state.set_files(files, cx));
                        view.memory = Some(memory);
                    }
                    Ok(Prepared::Raw(page)) => {
                        view.raw = Some(
                            cx.new(|_| crate::views::reader::paged::PagedReader::text_only(page)),
                        )
                    }
                    Err(error) => view.patch_error = Some(format!("{error:#}").into()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.load(self.roots.clone(), self.remote.clone(), cx);
    }

    fn area(area: Area) -> &'static str {
        match area {
            Area::Working => "Unstaged",
            Area::Staged => "Staged",
            Area::Untracked => "Untracked",
        }
    }

    pub(super) fn navigation(entity: &Entity<Self>, cx: &App) -> AnyElement {
        let view = entity.read(cx);
        let rows = view.rows.clone();
        let scroll = view.scroll.clone();
        let owner = entity.clone();
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .when(view.loading, |v| {
                v.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr("Loading changes…")),
                )
            })
            .children(view.errors.iter().enumerate().map(|(index, error)| {
                let error = error.clone();
                Button::new(("git-status-error", index))
                    .ghost()
                    .small()
                    .w_full()
                    .label(tr("Git query failed"))
                    .tooltip(error.clone())
                    .accessibility_label(tr("Copy error details"))
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(error.to_string()))
                    })
            }))
            .when(
                !view.loading
                    && view.repositories.iter().all(|r| r.changes.is_empty())
                    && view.errors.is_empty(),
                |v| {
                    v.child(
                        div()
                            .px_3()
                            .py_2()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr("No changes")),
                    )
                },
            )
            .child(
                uniform_list("git-changes-list", rows.len(), move |range, _, cx| {
                    let view = owner.read(cx);
                    range
                        .map(|index| {
                            let (root, change) = &rows[index];
                            match change {
                                None => div()
                                    .h(px(32.))
                                    .px_3()
                                    .flex()
                                    .items_center()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        root.file_name()
                                            .unwrap_or(root.as_os_str())
                                            .to_string_lossy()
                                            .into_owned(),
                                    )
                                    .into_any_element(),
                                Some(change) => {
                                    let selected = view.selection.as_ref().is_some_and(
                                        |(selected_root, selected)| {
                                            selected_root == root && selected == change
                                        },
                                    );
                                    let (root, change) = (root.clone(), change.clone());
                                    let click_owner = owner.clone();
                                    Button::new(("git-change", index))
                                        .ghost()
                                        .w_full()
                                        .h(px(32.))
                                        .px_3()
                                        .rounded(px(4.))
                                        .justify_start()
                                        .selected(selected)
                                        .tooltip(format!(
                                            "{} · {}",
                                            tr(Self::area(change.area)),
                                            root.join(&change.path).display()
                                        ))
                                        .accessibility_label(format!(
                                            "{} {} {}",
                                            change.status,
                                            change.path.display(),
                                            tr(Self::area(change.area))
                                        ))
                                        .child(
                                            h_flex()
                                                .w_full()
                                                .min_w_0()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .w(px(14.))
                                                        .flex_shrink_0()
                                                        .text_color(if change.status == 'D' {
                                                            cx.theme().danger
                                                        } else {
                                                            cx.theme().foreground
                                                        })
                                                        .child(change.status.to_string()),
                                                )
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .truncate()
                                                        .text_xs()
                                                        .child(
                                                            change
                                                                .path
                                                                .to_string_lossy()
                                                                .into_owned(),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .flex_shrink_0()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(tr(Self::area(change.area))),
                                                ),
                                        )
                                        .on_click(move |_, _, cx| {
                                            click_owner.update(cx, |view, cx| {
                                                cx.emit(ShowDiff);
                                                view.select(root.clone(), change.clone(), cx)
                                            })
                                        })
                                        .into_any_element()
                                }
                            }
                        })
                        .collect()
                })
                .track_scroll(&scroll)
                .flex_1()
                .min_h_0(),
            )
            .into_any_element()
    }
}

impl Render for Changes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.diff.read(cx).mode();
        let ready = !self.patch_loading && !self.diff.read(cx).files().is_empty();
        let toolbar = h_flex()
            .w_full()
            .min_w_0()
            .h(px(38.))
            .flex_shrink_0()
            .px_2()
            .gap_1()
            .child(div().flex_1().min_w_0().truncate().text_sm().child(
                self.selection.as_ref().map_or_else(
                    || tr("Changes").to_owned(),
                    |(_, change)| change.path.to_string_lossy().into_owned(),
                ),
            ))
            .child(
                Button::new("diff-open-file")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::FileCode)
                    .tooltip(tr("Open File"))
                    .accessibility_label(tr("Open File"))
                    .disabled(
                        self.selection
                            .as_ref()
                            .is_none_or(|(_, change)| change.status == 'D'),
                    )
                    .on_click(cx.listener(|view, _, _, cx| {
                        if let Some((root, change)) = &view.selection {
                            cx.emit(OpenFile(root.join(&change.path)));
                        }
                    })),
            )
            .child(
                Button::new("diff-unified")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::Rows2)
                    .selected(mode == DiffMode::Unified)
                    .tooltip(tr("Unified diff"))
                    .accessibility_label(tr("Unified diff"))
                    .disabled(!ready)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.diff
                            .update(cx, |state, cx| state.set_mode(DiffMode::Unified, cx))
                    })),
            )
            .child(
                Button::new("diff-split")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::Columns2)
                    .selected(mode == DiffMode::Split)
                    .tooltip(tr("Split diff"))
                    .accessibility_label(tr("Split diff"))
                    .disabled(!ready)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.diff
                            .update(cx, |state, cx| state.set_mode(DiffMode::Split, cx))
                    })),
            )
            .child(
                Button::new("diff-wrap")
                    .ghost()
                    .small()
                    .label(tr("Wrap"))
                    .selected(self.soft_wrap)
                    .disabled(!ready)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.soft_wrap = !view.soft_wrap;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("diff-previous")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::ArrowUp)
                    .tooltip(tr("Previous change"))
                    .accessibility_label(tr("Previous change"))
                    .disabled(!ready)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.diff.update(cx, |state, cx| state.previous_change(cx))
                    })),
            )
            .child(
                Button::new("diff-next")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::ArrowDown)
                    .tooltip(tr("Next change"))
                    .accessibility_label(tr("Next change"))
                    .disabled(!ready)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.diff.update(cx, |state, cx| state.next_change(cx))
                    })),
            )
            .child(
                Button::new("diff-refresh")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::RefreshCw)
                    .tooltip(tr("Refresh"))
                    .accessibility_label(tr("Refresh"))
                    .disabled(self.loading)
                    .on_click(cx.listener(|view, _, _, cx| view.refresh(cx))),
            );
        let content = if self.patch_loading || (self.loading && self.selection.is_none()) {
            message(tr("Loading changes…"), cx)
        } else if let Some(error) = self.patch_error.as_ref().or_else(|| {
            self.selection
                .is_none()
                .then(|| self.errors.first())
                .flatten()
        }) {
            let error = error.clone();
            let copied_error = error.clone();
            v_flex()
                .size_full()
                .min_h_0()
                .p_4()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .flex_1()
                        .min_h_0()
                        .id("diff-error-details")
                        .overflow_y_scroll()
                        .child(error),
                )
                .child(
                    Button::new("copy-diff-error")
                        .ghost()
                        .small()
                        .label(tr("Copy error details"))
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                copied_error.to_string(),
                            ))
                        }),
                )
                .child(
                    Button::new("retry-diff")
                        .small()
                        .label(tr("Retry"))
                        .on_click(cx.listener(|view, _, _, cx| view.refresh(cx))),
                )
                .into_any_element()
        } else if let Some(raw) = &self.raw {
            v_flex()
                .size_full()
                .min_h_0()
                .child(
                    div()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr("Showing patch source")),
                )
                .child(raw.clone())
                .into_any_element()
        } else if ready {
            Diff::new(&self.diff)
                .soft_wrap(self.soft_wrap)
                .size_full()
                .into_any_element()
        } else {
            message(
                tr(if self.selection.is_some() {
                    "No textual changes"
                } else {
                    "No changes"
                }),
                cx,
            )
        };
        v_flex()
            .size_full()
            .min_h_0()
            .child(toolbar)
            .child(div().flex_1().min_h_0().w_full().child(content))
    }
}

fn message(text: String, cx: &App) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.to_owned())
        .into_any_element()
}
