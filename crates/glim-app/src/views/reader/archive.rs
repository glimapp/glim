//! Archive directory requests and background task ownership.
mod view;

use crate::app::reader_jobs::{self, JobClass, Priority};
use glim_core::{archive::Archive, resources::ResourcePool};
use glim_services::{
    locale::tr,
    work::{CancelOnDrop, Cancellation},
};
use gpui_kit::*;
use std::sync::Arc;
use view::{ArchiveView, Navigate};

pub(super) struct ArchiveReader {
    archive: Arc<Archive>,
    resources: ResourcePool,
    view: Entity<ArchiveView>,
    active: bool,
    pending: Option<String>,
    task: Option<(CancelOnDrop, Task<()>)>,
    _subscription: Subscription,
}

impl ArchiveReader {
    pub fn new(archive: Archive, cx: &mut Context<Self>) -> Self {
        Self::with_resources(archive, glim_services::resources::pool().clone(), cx)
    }

    fn with_resources(archive: Archive, resources: ResourcePool, cx: &mut Context<Self>) -> Self {
        let archive = Arc::new(archive);
        let view = cx.new(|cx| ArchiveView::new(archive.clone(), tr, cx));
        let subscription = cx.subscribe(&view, |reader, _, Navigate(prefix), cx| {
            reader.navigate(prefix.clone(), cx);
        });
        Self {
            archive,
            resources,
            view,
            active: false,
            pending: Some(String::new()),
            task: None,
            _subscription: subscription,
        }
    }

    fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.active = active;
        if active {
            self.load(cx);
        } else {
            self.task = None;
        }
    }

    fn navigate(&mut self, prefix: String, cx: &mut Context<Self>) {
        self.pending = Some(prefix);
        self.task = None;
        self.load(cx);
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        if !self.active || self.task.is_some() {
            return;
        }
        let Some(prefix) = self.pending.clone() else {
            return;
        };
        self.view
            .update(cx, |view, cx| view.loading(prefix.clone(), cx));
        let archive = self.archive.clone();
        let resources = self.resources.clone();
        let (cancel, guard) = Cancellation::task();
        let work = reader_jobs::run_with_scheduler(
            cx.background_executor().clone(),
            reader_jobs::document_scheduler(cx),
            JobClass::Projection,
            Priority::Interactive,
            cancel.clone(),
            move |cancel| archive.directory_entries(&prefix, &resources, || cancel.check()),
        );
        let task = cx.spawn(async move |reader, cx| {
            let result = work.await;
            if cancel.check().is_err() {
                return;
            }
            let _ = reader.update(cx, |reader, cx| {
                reader.task = None;
                match result {
                    Ok(rows) => {
                        reader.pending = None;
                        reader.view.update(cx, |view, cx| view.publish(rows, cx));
                    }
                    Err(error) => {
                        let message = if glim_services::resources::is_memory_pressure(&error) {
                            tr("Not enough memory to display this folder.")
                        } else {
                            format!("{error:#}")
                        };
                        reader.view.update(cx, |view, cx| view.fail(message, cx));
                    }
                }
            });
        });
        self.task = Some((guard, task));
    }
}

impl crate::views::reader::contract::ReaderView for super::surface::SurfaceEntity<ArchiveReader> {
    fn kind(&self) -> &'static str {
        "zip"
    }
    fn render(&mut self, cx: &mut App) -> AnyElement {
        self.read(cx).view.clone().into_any_element()
    }
    fn set_active(&self, active: bool, cx: &mut App) {
        self.update(cx, |reader, cx| reader.set_active(active, cx));
    }
    fn operations(&self) -> crate::views::reader::contract::ReaderOperations<'_> {
        crate::views::reader::contract::ReaderOperations {
            archive: Some(self),
            ..Default::default()
        }
    }
}
impl super::surface::ReaderSurface for super::surface::SurfaceEntity<ArchiveReader> {}
impl crate::views::reader::contract::ArchiveControls
    for super::surface::SurfaceEntity<ArchiveReader>
{
    fn set_extract_action(&self, label: String, action: super::ArchiveAction, cx: &mut App) {
        self.read(cx).view.clone().update(cx, |view, _| {
            view.extract = Some((label, action));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{ArchiveReader, Navigate};
    use crate::{
        app::reader_jobs,
        views::reader::{contract::ReaderView, surface::SurfaceEntity},
    };
    use glim_core::{
        archive::{Archive, DirectoryEntry, Entry},
        resources::ResourcePool,
    };
    use gpui_kit::{AppContext, TestAppContext};
    use std::path::Path;

    #[gpui_kit::test]
    fn directory_pressure_leaves_the_target_available_for_retry(cx: &mut TestAppContext) {
        let archive = Archive::new(
            Path::new("fixture.zip"),
            "ZIP".into(),
            vec![Entry {
                name: "item".into(),
                size: 1,
                directory: false,
            }],
        );
        let bytes = (size_of::<DirectoryEntry>() + "item".len()) as u64;
        let pool = ResourcePool::with_budgets(0, bytes);
        let obstruction = pool.reserve_working(1, 1).unwrap();
        let (reader, view) = cx.update(|cx| {
            reader_jobs::isolate_document_scheduler(cx);
            let reader = cx.new(|cx| ArchiveReader::with_resources(archive, pool.clone(), cx));
            let view = reader.read(cx).view.clone();
            (SurfaceEntity(reader), view)
        });
        cx.update(|cx| reader.set_active(true, cx));
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(view.read(cx).has_failed());
            assert_eq!(reader.read(cx).pending.as_deref(), Some(""));
        });

        drop(obstruction);
        cx.update(|cx| view.update(cx, |_, cx| cx.emit(Navigate(String::new()))));
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(view.read(cx).directory().1.unwrap()[0].name, "item");
            assert!(reader.read(cx).pending.is_none());
        });
        assert_eq!(pool.snapshot().working_bytes, bytes);
    }

    #[gpui_kit::test]
    fn replacing_and_hiding_directory_queries_only_publish_the_latest_target(
        cx: &mut TestAppContext,
    ) {
        let entries = ["first/inner/a.txt", "second/b.txt"]
            .into_iter()
            .map(|name| Entry {
                name: name.into(),
                size: 1,
                directory: false,
            })
            .collect();
        let archive = Archive::new(Path::new("fixture.zip"), "ZIP".into(), entries);
        let (reader, view) = cx.update(|cx| {
            reader_jobs::isolate_document_scheduler(cx);
            let reader = cx.new(|cx| ArchiveReader::new(archive, cx));
            let view = reader.read(cx).view.clone();
            (SurfaceEntity(reader), view)
        });

        cx.update(|cx| reader.set_active(true, cx));
        cx.run_until_parked();
        cx.update(|cx| {
            let (prefix, rows) = view.read(cx).directory();
            assert_eq!(prefix, "");
            assert_eq!(rows.unwrap().len(), 2);
            view.update(cx, |_, cx| cx.emit(Navigate("first/inner/".into())));
        });
        cx.update(|cx| {
            assert_eq!(view.read(cx).directory(), ("first/inner/", None));
            view.update(cx, |view, cx| view.up(cx));
        });
        cx.update(|cx| {
            assert_eq!(view.read(cx).directory(), ("first/", None));
            view.update(cx, |view, cx| view.up(cx));
        });
        cx.update(|cx| {
            assert_eq!(view.read(cx).directory(), ("", None));
            view.update(cx, |_, cx| cx.emit(Navigate("second/".into())));
        });
        cx.update(|cx| reader.set_active(false, cx));
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(view.read(cx).directory(), ("second/", None));
            assert!(reader.read(cx).task.is_none());
        });

        cx.update(|cx| reader.set_active(true, cx));
        cx.run_until_parked();
        cx.update(|cx| {
            let (prefix, rows) = view.read(cx).directory();
            assert_eq!(prefix, "second/");
            assert_eq!(rows.unwrap()[0].name, "b.txt");
            assert!(reader.read(cx).pending.is_none());
        });
    }
}
