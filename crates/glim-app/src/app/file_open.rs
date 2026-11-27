//! Route external open requests and reuse existing document tabs.
use super::OpenWorkspaces;
use gpui_kit::*;
use std::path::Path;

fn open_in_workspace(path: &Path, cx: &mut App) -> bool {
    let workspaces = OpenWorkspaces::windows(cx);
    let local = |view: &WeakEntity<super::Workspace>| {
        let view = view.upgrade()?;
        (!glim_services::remote::is_remote_id(&view.read(cx).workspace_descriptor().id))
            .then_some(view)
    };
    let Some((destination, handle)) = workspaces.iter().find_map(|(view, handle)| {
        let view = local(view)?;
        view.read(cx)
            .workspace_descriptor()
            .folders
            .iter()
            .any(|root| path.starts_with(root))
            .then_some((view, *handle))
    }) else {
        return false;
    };
    let source = workspaces.iter().find_map(|(view, handle)| {
        let view = local(view)?;
        view.read(cx)
            .open_document_paths()
            .iter()
            .any(|opened| opened == path)
            .then_some((view, *handle))
    });
    let mut moved = None;
    if !destination
        .read(cx)
        .open_document_paths()
        .iter()
        .any(|opened| opened == path)
        && let Some((source, source_handle)) = source
        && source != destination
    {
        if destination.read(cx).update_activity(cx).loading {
            return reveal_existing_file(path, None, None, cx);
        }
        // A busy reader stays in its original workspace until it can move safely.
        // Reuse it there instead of creating a second editor for the same file.
        moved = source_handle
            .update(cx, |_, window, cx| {
                source.update(cx, |view, cx| view.take_file_tab(path, window, cx))
            })
            .ok()
            .flatten();
        if moved.is_none() {
            return reveal_existing_file(path, None, None, cx);
        }
    }
    // Both entities and their windows remain alive during this synchronous handoff.
    handle
        .update(cx, |_, window, cx| {
            destination.update(cx, |view, cx| {
                if let Some(reader) = moved {
                    view.receive_system_tab(reader, window, cx);
                } else {
                    view.open_system_file(path.to_path_buf(), window, cx);
                }
            });
            window.activate_window();
        })
        .expect("Existing external-file destination window");
    cx.activate(true);
    true
}

pub(crate) fn reveal_existing_file(
    path: &Path,
    exclude: Option<EntityId>,
    offset: Option<u64>,
    cx: &mut App,
) -> bool {
    let windows = OpenWorkspaces::windows(cx);
    windows
        .into_iter()
        .filter(|(view, _)| Some(view.entity_id()) != exclude)
        .any(|(view, handle)| {
            handle
                .update(cx, |_, window, cx| {
                    view.update(cx, |v, cx| {
                        let found = v.reveal_file_path(path, window, cx);
                        if found && let Some(offset) = offset {
                            v.reveal_content_at(path, offset, window, cx);
                        }
                        found
                    })
                    .unwrap_or(false)
                })
                .unwrap_or(false)
        })
}

pub(super) fn open_system_files(urls: Vec<String>, cx: &mut App) {
    super::updates::interrupt_for_files(cx);
    for value in urls {
        let Ok(url) = url::Url::parse(&value) else {
            continue;
        };
        if super::remote_open::receive(&url, cx) || super::finder_integration::receive(&url, cx) {
            continue;
        }
        if url.scheme() != "file" {
            continue;
        }
        let Ok(path) = url.to_file_path() else {
            continue;
        };
        if open_in_workspace(&path, cx) || reveal_existing_file(&path, None, None, cx) {
            continue;
        }
        let windows = OpenWorkspaces::windows(cx);
        let reused = windows.iter().rev().any(|(view, handle)| {
            handle
                .update(cx, |_, window, cx| {
                    view.update(cx, |v, cx| {
                        v.is_visible_workspace(cx) && v.open_system_file(path.clone(), window, cx)
                    })
                    .unwrap_or(false)
                })
                .unwrap_or(false)
        });
        if reused {
            cx.activate(true);
        } else {
            super::open_workspace(Some(path), cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::open_in_workspace;
    use crate::app::OpenWorkspaces;
    use crate::views::workspace_window::WorkspaceWindow;
    use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext, WindowOptions};

    fn workspaces(
        paths: &[std::path::PathBuf],
        cx: &mut TestAppContext,
    ) -> (AnyWindowHandle, Entity<WorkspaceWindow>) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::app::remote::init(Default::default(), cx);
            cx.set_global(OpenWorkspaces::default());
            crate::app::reader_jobs::isolate_document_scheduler(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                let host = cx.new(WorkspaceWindow::new);
                host.update(cx, |host, cx| {
                    for path in paths {
                        host.append(Some(path.clone()), None, window, cx);
                    }
                });
                host
            })
            .unwrap()
        })
    }

    #[gpui_kit::test]
    fn external_file_uses_a_containing_workspace_instead_of_the_visible_one(
        cx: &mut TestAppContext,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let first = root.join("first");
        let nested = first.join("nested");
        let other = root.join("other");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir(&other).unwrap();
        let path = nested.join("external.txt");
        std::fs::write(&path, "opened from Codex").unwrap();
        let (_, host) = workspaces(&[first, nested, other], cx);
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(host.read(cx).active, 2);
            assert!(open_in_workspace(&path, cx));
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let host = host.read(cx);
            assert!(host.active < 2);
            assert_eq!(
                host.slots[host.active].view.read(cx).open_document_paths(),
                [path.clone()]
            );
            assert!(host.slots[2].view.read(cx).open_document_paths().is_empty());
            assert!(open_in_workspace(&path, cx));
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(crate::app::open_document_paths(None, cx), [path]);
        });
    }

    #[gpui_kit::test]
    fn existing_external_tab_moves_to_its_workspace_without_duplicating_other_tabs(
        cx: &mut TestAppContext,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let owner = root.join("owner");
        let other = root.join("other");
        std::fs::create_dir(&owner).unwrap();
        std::fs::create_dir(&other).unwrap();
        let path = owner.join("external.txt");
        let retained = other.join("retained.txt");
        std::fs::write(&path, "external contents").unwrap();
        std::fs::write(&retained, "another tab").unwrap();
        let (handle, host) = workspaces(&[owner, other], cx);
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            let source = host.read(cx).slots[1].view.clone();
            source.update(cx, |view, cx| {
                view.open_system_file(retained.clone(), window, cx);
                view.open_system_file(path.clone(), window, cx);
            });
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| assert!(open_in_workspace(&path, cx)));
        cx.run_until_parked();
        cx.update(|cx| {
            let host = host.read(cx);
            assert_eq!(host.active, 0);
            assert_eq!(host.slots[0].view.read(cx).open_document_paths(), [path]);
            assert_eq!(
                host.slots[1].view.read(cx).open_document_paths(),
                [retained]
            );
        });
    }

    #[gpui_kit::test]
    fn collecting_paths_during_trash_skips_the_borrowed_workspace(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let archive = root.join("opened.zip");
        let same_window = root.join("same-window.txt");
        let other_window = root.join("other-window.txt");
        // An empty ZIP end-of-central-directory record is a valid archive.
        let mut zip = [0u8; 22];
        zip[..4].copy_from_slice(b"PK\x05\x06");
        std::fs::write(&archive, zip).unwrap();
        std::fs::write(&same_window, "same window").unwrap();
        std::fs::write(&other_window, "other window").unwrap();
        let (handle, host) = workspaces(&[archive.clone(), same_window.clone()], cx);
        let (_, second_host) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                let host = cx.new(WorkspaceWindow::new);
                host.update(cx, |host, cx| {
                    host.append(Some(other_window.clone()), None, window, cx);
                });
                host
            })
            .unwrap()
        });
        cx.run_until_parked();
        let current = cx.update(|cx| host.read(cx).slots[0].view.clone());
        cx.update_window(handle, |_, _, cx| {
            current.update(cx, |view, cx| {
                let mut paths = view.all_open_document_paths(cx);
                paths.sort();
                let mut expected = vec![archive.clone(), same_window.clone(), other_window.clone()];
                expected.sort();
                assert_eq!(paths, expected);
            });
        })
        .unwrap();
        cx.update(|cx| {
            assert_eq!(second_host.read(cx).slots.len(), 1);
            let mut paths = crate::app::open_document_paths(None, cx);
            paths.sort();
            let mut expected = vec![archive, same_window, other_window];
            expected.sort();
            assert_eq!(paths, expected);
        });
    }
}
