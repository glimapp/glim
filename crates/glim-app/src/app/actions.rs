gpui_kit::actions!(
    glim,
    [
        NewWindow,
        NewFile,
        OpenSettings,
        About,
        CheckForUpdates,
        LanguageSystem,
        LanguageChinese,
        LanguageEnglish,
        KeyboardShortcuts,
        SearchWorkspace,
        ToggleSidebar,
        TogglePreview,
        ToggleWordWrap,
        NextTab,
        PreviousTab,
        NavigateBack,
        NavigateForward,
        CloseWorkspace,
        DefaultApplications,
        ToggleDarkMode,
        Quit,
        OpenFile,
        OpenFolder,
        AddFolder,
        SaveFile,
        Refresh,
        CloseWindow
    ]
);

pub(crate) use crate::views::{
    BookContents, BookNextChapter, BookNextPage, BookPreviousChapter, BookPreviousPage,
};
