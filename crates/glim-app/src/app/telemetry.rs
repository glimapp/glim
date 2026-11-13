use glim_services::telemetry::Consent;
use gpui_kit::*;

#[derive(Default)]
pub(crate) struct Telemetry {
    pub consent: Consent,
    pub persistence_error: bool,
    pub saving: bool,
    report: Option<Task<()>>,
    cancel: Option<glim_services::work::CancelOnDrop>,
    save: Option<Task<()>>,
    started: Option<std::time::Instant>,
    save_revision: u64,
}
impl Global for Telemetry {}

pub(crate) fn initialize(stored: anyhow::Result<Option<Consent>>, cx: &mut App) {
    let persistence_error = stored.is_err();
    let consent = match stored {
        Ok(Some(consent)) => consent,
        Ok(None) => Consent {
            usage: true,
            diagnostics: true,
        },
        Err(_) => Consent::default(),
    };
    crate::views::set_native_telemetry_consent(consent);
    cx.set_global(Telemetry {
        consent,
        persistence_error,
        ..Default::default()
    });
    if consent.any() {
        start("app_started", cx);
    }
}
fn start(event: &'static str, cx: &mut App) {
    let consent = cx.global::<Telemetry>().consent;
    let worker = glim_services::telemetry::start(env!("GLIM_BUILD_NUMBER"), consent);
    glim_services::telemetry::track(
        event,
        [(
            "updater_enabled",
            glim_services::telemetry::flag(cfg!(feature = "macos-updater")),
        )],
    );
    let (cancel, guard) = glim_services::work::Cancellation::task();
    let task = cx.background_executor().spawn(async move {
        worker.run(&cancel);
    });
    let state = cx.global_mut::<Telemetry>();
    state.started = Some(std::time::Instant::now());
    state.cancel = Some(guard);
    state.report = Some(task);
}
pub(crate) fn set_usage(enabled: bool, cx: &mut App) {
    let mut consent = cx.global::<Telemetry>().consent;
    consent.usage = enabled;
    set_consent(consent, cx);
}
pub(crate) fn set_diagnostics(enabled: bool, cx: &mut App) {
    let mut consent = cx.global::<Telemetry>().consent;
    consent.diagnostics = enabled;
    set_consent(consent, cx);
}
pub(crate) fn set_consent(consent: Consent, cx: &mut App) {
    let changed = cx.global::<Telemetry>().consent != consent;
    if changed {
        // Invalidate native aggregates and cancel the old worker before either
        // channel starts again. Revoked data cannot survive a consent change.
        crate::views::set_native_telemetry_consent(consent);
        glim_services::telemetry::stop(false);
        let state = cx.global_mut::<Telemetry>();
        state.cancel.take();
        state.report.take();
        state.started = None;
        state.consent = consent;
        if consent.any() {
            start("telemetry_enabled", cx);
        }
        for (view, _) in super::OpenWorkspaces::windows(cx) {
            let _ = view.update(cx, |view, _| view.reset_usage());
        }
    }
    persist(cx);
}
pub(crate) fn persist(cx: &mut App) {
    let state = cx.global_mut::<Telemetry>();
    state.saving = true;
    state.persistence_error = false;
    state.save_revision += 1;
    let revision = state.save_revision;
    let consent = state.consent;
    let previous = state.save.take();
    let task = cx.spawn(async move |cx| {
        if let Some(previous) = previous {
            previous.await;
        }
        let result = cx
            .background_executor()
            .spawn(async move { glim_services::preferences::save_telemetry_consent(consent) })
            .await;
        if let Err(error) = &result {
            log::warn!("Cannot save telemetry preferences: {error:#}");
        }
        cx.update(|cx| {
            let state = cx.global_mut::<Telemetry>();
            if state.save_revision == revision {
                state.saving = false;
                state.persistence_error = result.is_err();
            }
            cx.refresh_windows();
        });
    });
    cx.global_mut::<Telemetry>().save = Some(task);
    cx.refresh_windows();
}
pub(crate) fn shutdown(cx: &mut App) -> Option<Task<()>> {
    let state = cx.global_mut::<Telemetry>();
    if state.consent.usage {
        glim_services::telemetry::track(
            "app_exited",
            [(
                "session_duration_s",
                glim_services::telemetry::number(
                    state.started.map_or(0, |start| start.elapsed().as_secs()),
                ),
            )],
        );
    }
    glim_services::telemetry::stop(true);
    let report = state.report.take();
    let cancel = state.cancel.take();
    let save = state.save.take();
    if report.is_none() && save.is_none() {
        return None;
    }
    Some(cx.spawn(async move |cx| {
        // Normal quit gets a short flush window. Opt-out cancels immediately.
        let executor = cx.background_executor().clone();
        let timeout = cx.background_executor().spawn(async move {
            executor.timer(std::time::Duration::from_secs(2)).await;
            drop(cancel);
        });
        if let Some(report) = report {
            report.await;
        }
        drop(timeout);
        if let Some(save) = save {
            save.await;
        }
    }))
}

pub(crate) fn observe_commands(cx: &mut App) {
    cx.observe_keystrokes(|event, window, cx| {
        if !cx.global::<Telemetry>().consent.usage {
            return;
        }
        let Some(action) = &event.action else {
            return;
        };
        // Resolved compile-time command names only. Never log typed keys,
        // text, input context values or clipboard contents.
        use glim_services::telemetry::{text, track};
        let modifiers = event.keystroke.modifiers;
        let mut props = Vec::new();
        for (view, handle) in super::OpenWorkspaces::windows(cx) {
            if handle.window_id() == window.window_handle().window_id()
                && let Some(view) = view.upgrade()
                && view.read(cx).is_visible_workspace(cx)
            {
                props.extend(view.read(cx).usage_context());
                break;
            }
        }
        props.extend(glim_services::telemetry::shortcut_properties(
            &event.keystroke.key,
            modifiers.platform,
            modifiers.control,
            modifiers.alt,
            modifiers.shift,
        ));
        props.push(("action", text(action.name())));
        props.push(("key_basis", text("resolved_binding")));
        track("shortcut_used", props);
    })
    .detach();
}
