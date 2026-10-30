//! Finder extension setup and serialized workspace URL delivery.
#![allow(unsafe_code)]
use gpui_kit::*;

unsafe extern "C" {
    fn glim_finder_manage();
}

#[derive(Default)]
struct FinderRequests(Option<Task<()>>);
impl Global for FinderRequests {}

pub fn init(cx: &mut App) {
    cx.set_global(FinderRequests::default());
}

pub fn manage(cx: &mut App) {
    cx.defer(|_| {
        // SAFETY: system extension management must be opened on the UI thread.
        unsafe { glim_finder_manage() };
    });
}

pub fn receive(url: &url::Url, cx: &mut App) -> bool {
    if url.scheme() != "glim" {
        return false;
    }
    if url.host_str() != Some("open-workspace") {
        return true;
    }
    let Some(path) = url.query_pairs().find_map(|(name, value)| {
        (name == "path").then(|| std::path::PathBuf::from(value.as_ref()))
    }) else {
        return true;
    };
    if !path.is_absolute() {
        return true;
    }
    let previous = cx.global_mut::<FinderRequests>().0.take();
    let task = cx.spawn(async move |cx| {
        if let Some(previous) = previous {
            previous.await;
        }
        let result = cx
            .background_executor()
            .spawn(async move {
                if !path.is_dir() || glim_services::preview::is_iwork(&path) {
                    return None;
                }
                let recent = glim_services::preferences::recent_workspaces()
                    .unwrap_or_default()
                    .into_iter()
                    .find(|workspace| workspace.folders.as_slice() == [path.clone()]);
                Some((path, recent))
            })
            .await;
        if let Some((path, recent)) = result {
            cx.update(|cx| super::open_finder_workspace(path, recent, cx));
        }
    });
    cx.global_mut::<FinderRequests>().0 = Some(task);
    true
}
