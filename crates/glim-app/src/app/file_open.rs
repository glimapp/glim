//! Route external local files to a workspace that contains them.
use super::OpenWorkspaces;
use gpui_kit::*;
use std::path::Path;

pub(super) fn open_in_workspace(path: &Path, cx: &mut App) -> bool {
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
            return super::reveal_existing_file(path, None, cx);
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
            return super::reveal_existing_file(path, None, cx);
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
            assert_eq!(crate::app::open_document_paths(cx), [path]);
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
}
