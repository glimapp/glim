use super::media::{GifReader, InlineImageReader};
use crate::app::reader_jobs::{self, JobClass, Priority};
use glim_services::markup_images::{self, PictureSources};
use gpui_kit::*;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
pub(super) struct Pictures {
    pub sources: Arc<HashMap<String, PictureSources>>,
    pub animations: Arc<HashMap<String, Entity<GifReader>>>,
    pub stills: Arc<HashMap<String, Entity<InlineImageReader>>>,
    pub requested: Arc<Mutex<HashSet<String>>>,
    animation_subscriptions: Vec<Subscription>,
    still_subscriptions: Vec<Subscription>,
    base: url::Url,
    active: bool,
    task: Option<Task<()>>,
}
impl Pictures {
    pub fn new(text: String, html: bool, base: url::Url, cx: &mut Context<Self>) -> Self {
        let mut pictures = Self {
            sources: Arc::new(HashMap::new()),
            animations: Arc::new(HashMap::new()),
            animation_subscriptions: Vec::new(),
            stills: Arc::new(HashMap::new()),
            requested: Arc::new(Mutex::new(HashSet::new())),
            still_subscriptions: Vec::new(),
            base,
            active: true,
            task: None,
        };
        pictures.load(text, html, cx);
        pictures
    }
    pub fn load(&mut self, text: String, html: bool, cx: &mut Context<Self>) {
        self.sources = Arc::new(HashMap::new());
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let work = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Projection,
            Priority::Interactive,
            cancel,
            move |_cancel| {
                let sources = markup_images::picture_sources(&text, html);
                let references = markup_images::image_uris(&text, html);
                let mut images = references.clone();
                for variants in sources.values() {
                    images.extend(variants.light.iter().chain(variants.dark.iter()).cloned());
                }
                Ok((sources, images))
            },
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let Ok((sources, images)) = result else {
                let _ = view.update(cx, |view, _| view.task = None);
                return;
            };
            let _ = view.update(cx, |view, cx| {
                let mut animations = HashMap::new();
                let mut stills = HashMap::new();
                view.still_subscriptions.clear();
                view.animation_subscriptions.clear();
                for uri in images {
                    let Ok(url) = view.base.join(&uri) else {
                        continue;
                    };
                    if !matches!(url.scheme(), "file" | "http" | "https") {
                        continue;
                    }
                    let key = url.to_string();
                    if !url.path().to_ascii_lowercase().ends_with(".gif") {
                        if stills.contains_key(&key) {
                            continue;
                        }
                        let image = view
                            .stills
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| cx.new(|cx| InlineImageReader::new(url, cx)));
                        view.still_subscriptions
                            .push(cx.observe(&image, |_, _, cx| cx.notify()));
                        stills.insert(key, image);
                        continue;
                    }
                    if animations.contains_key(&key) {
                        continue;
                    }
                    let animation = view
                        .animations
                        .get(&key)
                        .cloned()
                        .unwrap_or_else(|| cx.new(|cx| GifReader::from_url(url, cx)));
                    view.animation_subscriptions
                        .push(cx.observe(&animation, |_, _, cx| cx.notify()));
                    animations.insert(key, animation);
                }
                view.animations = Arc::new(animations);
                view.sources = Arc::new(sources);
                view.stills = Arc::new(stills);
                view.set_active(view.active, cx);
                view.task = None;
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub fn refresh_local_images(&mut self, cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        for image in self.stills.values() {
            image.update(cx, |image, cx| image.refresh(cx));
        }
    }
    pub fn set_active(&mut self, active: bool, cx: &mut App) {
        self.active = active;
        if !active {
            self.finish_frame(&HashSet::new(), cx);
        }
    }
    fn finish_frame(&mut self, requested: &HashSet<String>, cx: &mut App) {
        for (uri, animation) in self.animations.iter() {
            if !self.active || !requested.contains(uri) {
                animation.update(cx, |animation, cx| animation.set_active(false, cx));
            }
        }
        for (uri, image) in self.stills.iter() {
            if !self.active || !requested.contains(uri) {
                image.update(cx, |image, cx| image.set_active(false, cx));
            }
        }
    }
}

/// Collect actual image requests from the virtualized subtree each painted frame.
pub(super) struct PictureViewport {
    pub owner: Entity<Pictures>,
    pub content: AnyElement,
}
impl IntoElement for PictureViewport {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for PictureViewport {
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
        self.owner.read(cx).requested.lock().unwrap().clear();
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
        let requested = std::mem::take(&mut *self.owner.read(cx).requested.lock().unwrap());
        let owner = self.owner.downgrade();
        cx.defer(move |cx| {
            let _ = owner.update(cx, |view, cx| view.finish_frame(&requested, cx));
        });
    }
}
