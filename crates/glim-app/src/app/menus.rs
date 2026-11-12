use super::actions::{
    About, AddFolder, CheckForUpdates, CloseWindow, CloseWorkspace, KeyboardShortcuts,
    NavigateBack, NavigateForward, NewFile, NewWindow, NextTab, OpenFile, OpenFolder, OpenRemote,
    OpenSettings, PreviousTab, Quit, Refresh, SaveFile, SearchWorkspace, ToggleFileTree,
    TogglePreview, ToggleSidebar, ToggleWordWrap,
};
use glim_services::locale::tr;
use gpui_kit::{
    App, Menu, MenuItem, OsAction,
    component::input::{Copy, Cut, Paste, Redo, Replace, Search, SelectAll, Undo},
};

pub(super) fn update(cx: &mut App) {
    cx.set_dock_menu(vec![MenuItem::action(tr("New Window"), NewWindow)]);
    cx.set_menus(vec![
        Menu {
            name: "Glim".into(),
            disabled: false,
            items: vec![
                MenuItem::action(tr("About Glim"), About),
                MenuItem::action(
                    tr(match super::updates::check_label(cx) {
                        "Check…" => "Check for Updates…",
                        label => label,
                    }),
                    CheckForUpdates,
                ),
                MenuItem::separator(),
                MenuItem::action(tr("Settings…"), OpenSettings),
                MenuItem::action(tr("Keyboard Shortcuts…"), KeyboardShortcuts),
                MenuItem::separator(),
                MenuItem::action(tr("Quit Glim"), Quit),
            ],
        },
        Menu {
            name: tr("File").into(),
            disabled: false,
            items: vec![
                MenuItem::action(tr("New File"), NewFile),
                MenuItem::action(tr("New Window"), NewWindow),
                MenuItem::separator(),
                MenuItem::action(tr("Open File…"), OpenFile),
                MenuItem::action(tr("Open Folder…"), OpenFolder),
                MenuItem::action(tr("Open Remote Workspace…"), OpenRemote),
                MenuItem::action(tr("Add Folder to Workspace…"), AddFolder),
                MenuItem::separator(),
                MenuItem::action(tr("Save"), SaveFile),
                MenuItem::separator(),
                MenuItem::action(tr("Close Tab"), CloseWindow),
                MenuItem::action(tr("Close Window"), CloseWorkspace),
            ],
        },
        Menu {
            name: tr("Edit").into(),
            disabled: false,
            items: vec![
                MenuItem::os_action(tr("Undo"), Undo, OsAction::Undo),
                MenuItem::os_action(tr("Redo"), Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action(tr("Cut"), Cut, OsAction::Cut),
                MenuItem::os_action(tr("Copy"), Copy, OsAction::Copy),
                MenuItem::os_action(tr("Paste"), Paste, OsAction::Paste),
                MenuItem::os_action(tr("Select All"), SelectAll, OsAction::SelectAll),
                MenuItem::separator(),
                MenuItem::action(tr("Find in file"), Search),
                MenuItem::action(tr("Find and Replace"), Replace),
                MenuItem::action(tr("Search workspace files"), SearchWorkspace),
            ],
        },
        Menu {
            name: tr("View").into(),
            disabled: false,
            items: vec![
                MenuItem::action(tr("Toggle Sidebar"), ToggleSidebar),
                MenuItem::action(tr("Toggle File Tree"), ToggleFileTree),
                MenuItem::separator(),
                MenuItem::action(tr("Toggle Preview"), TogglePreview),
                MenuItem::action(tr("Word wrap"), ToggleWordWrap),
                MenuItem::separator(),
                MenuItem::action(tr("Next tab"), NextTab),
                MenuItem::action(tr("Previous tab"), PreviousTab),
                MenuItem::action(tr("Go Back"), NavigateBack),
                MenuItem::action(tr("Go Forward"), NavigateForward),
                MenuItem::separator(),
                MenuItem::action(tr("Refresh"), Refresh),
            ],
        },
    ]);
}
