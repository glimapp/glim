//! Internal static-module contract. No application globals or service payloads.
use gpui_kit::*;
use std::sync::Arc;

pub type ArchiveAction = Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>;

#[derive(Clone)]
pub struct ReaderHost {
    pub translate: fn(&str) -> String,
    pub action: fn(&'static str, &'static str, &[ReaderMetric]),
    pub resources: glim_core::resources::ResourcePool,
}

pub trait CopyImage {
    fn copy_image(&self, cx: &mut App) -> bool;
}

pub trait ArchiveControls {
    fn set_extract_action(&self, label: String, action: ArchiveAction, cx: &mut App);
}

pub trait PlaybackState {
    fn playlist_shuffle(&self, cx: &App) -> Option<bool>;
}

pub trait ReaderPersistence {
    fn is_persisting(&self) -> bool;
    fn take_write(&self, cx: &mut App) -> Option<Task<()>>;
}

#[derive(Default)]
pub struct ReaderOperations<'a> {
    pub image_copy: Option<&'a dyn CopyImage>,
    pub archive: Option<&'a dyn ArchiveControls>,
    pub playback: Option<&'a dyn PlaybackState>,
    pub persistence: Option<&'a dyn ReaderPersistence>,
}

pub trait ReaderView {
    fn can_find(&self, _cx: &App) -> bool {
        false
    }
    fn find(&self, _window: &mut Window, _cx: &mut App) {}
    fn kind(&self) -> &'static str;
    fn render(&mut self, cx: &mut App) -> AnyElement;
    fn operations(&self) -> ReaderOperations<'_> {
        ReaderOperations::default()
    }
    fn can_move_to_window(&self, _cx: &App) -> bool {
        true
    }
    fn bind_window(&self, _window: &mut Window, _cx: &mut App) {}
    fn set_active(&self, _active: bool, _cx: &mut App) {}
    fn is_unavailable(&self) -> bool {
        false
    }
}

/// Bounded numeric metadata; document contents and arbitrary text stay outside the host contract.
pub enum ReaderMetric {
    Flag(&'static str, bool),
    Count(&'static str, u64),
}
