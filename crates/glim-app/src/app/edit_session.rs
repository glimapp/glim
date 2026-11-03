//! Shared editing state; source, markup and page views retain the same session.
use gpui_kit::{
    component::input::{EditorState, InputEvent},
    *,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

#[derive(Clone)]
pub(crate) struct EditSession(Rc<State>);
struct State {
    editor: Entity<EditorState>,
    baseline: Rc<RefCell<Arc<str>>>,
    dirty: Rc<Cell<bool>>,
    revision: Rc<Cell<u64>>,
    readonly: bool,
    locked: Cell<bool>,
    memory: glim_services::resources::ResourceLease,
    _subscription: Subscription,
}
impl EditSession {
    pub fn new(editor: Entity<EditorState>, text: &str, readonly: bool, cx: &mut App) -> Self {
        let baseline = Rc::new(RefCell::new(Arc::<str>::from(if readonly {
            ""
        } else {
            text
        })));
        let dirty = Rc::new(Cell::new(false));
        let revision = Rc::new(Cell::new(0u64));
        let saved = baseline.clone();
        let changed = dirty.clone();
        let version = revision.clone();
        let memory = glim_services::resources::pool().track(
            (text.len() as u64).saturating_add(baseline.borrow().len() as u64),
            glim_services::resources::ResourceClass::State,
        );
        let tracked = memory.clone();
        let subscription = cx.subscribe(&editor, move |editor, event, cx| {
            if matches!(event, InputEvent::Change) {
                version.set(version.get().wrapping_add(1));
                let text = editor.read(cx).text();
                let baseline = saved.borrow();
                let is_dirty = !readonly && text != baseline.as_ref();
                tracked.resize((text.len() as u64).saturating_add(baseline.len() as u64));
                if changed.replace(is_dirty) != is_dirty {
                    cx.refresh_windows();
                }
            }
        });
        Self(Rc::new(State {
            editor,
            baseline,
            dirty,
            revision,
            readonly,
            locked: Cell::new(false),
            memory,
            _subscription: subscription,
        }))
    }
    pub fn editor(&self) -> &Entity<EditorState> {
        &self.0.editor
    }
    pub fn is_readonly(&self) -> bool {
        self.0.readonly
    }
    pub fn is_locked(&self) -> bool {
        self.0.locked.get()
    }
    pub fn is_dirty(&self) -> bool {
        self.0.dirty.get()
    }
    pub fn revision(&self) -> u64 {
        self.0.revision.get()
    }
    pub fn set_locked(&self, locked: bool, cx: &mut App) {
        self.0.locked.set(locked);
        self.0.editor.update(cx, |editor, cx| {
            editor.set_readonly(self.0.readonly || locked, cx)
        });
    }
    pub fn mark_saved(&self, text: impl Into<Arc<str>>, cx: &App) {
        let text = text.into();
        let current = self.0.editor.read(cx).text();
        self.0
            .dirty
            .set(!self.0.readonly && current != text.as_ref());
        self.0
            .memory
            .resize((current.len() as u64).saturating_add(text.len() as u64));
        *self.0.baseline.borrow_mut() = text;
    }
}
