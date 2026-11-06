use gpui_kit::*;

#[derive(Default)]
pub(crate) struct Telemetry {
    pub enabled: bool,
    report: Option<Task<()>>,
    cancel: Option<glim_services::work::CancelOnDrop>,
    save: Option<Task<()>>,
    started: Option<std::time::Instant>,
}
impl Global for Telemetry {}

pub(crate) fn initialize(cx: &mut App) {
    let enabled = glim_services::preferences::telemetry_enabled();
    crate::views::set_native_telemetry_enabled(enabled);
    cx.set_global(Telemetry {
        enabled,
        ..Default::default()
    });
    if enabled {
        start("app_started", cx);
    }
}
fn start(event: &'static str, cx: &mut App) {
    let worker = glim_services::telemetry::start(env!("GLIM_BUILD_NUMBER"));
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
pub(crate) fn set_enabled(enabled: bool, cx: &mut App) {
    let state = cx.global_mut::<Telemetry>();
    if state.enabled == enabled {
        return;
    }
    state.enabled = enabled;
    crate::views::set_native_telemetry_enabled(enabled);
    glim_services::telemetry::stop(false);
    state.cancel.take();
    state.report.take();
    let previous = state.save.take();
    let task = cx.spawn(async move |cx| {
        if let Some(previous) = previous {
            previous.await;
        }
        cx.background_executor()
            .spawn(async move {
                let _ = glim_services::preferences::save_telemetry_enabled(enabled);
            })
            .await;
    });
    cx.global_mut::<Telemetry>().save = Some(task);
    if enabled {
        start("telemetry_enabled", cx);
    }
    for (view, _) in super::OpenWorkspaces::windows(cx) {
        let _ = view.update(cx, |view, _| view.reset_usage());
    }
    cx.refresh_windows();
}
pub(crate) fn shutdown(cx: &mut App) -> Option<Task<()>> {
    let state = cx.global_mut::<Telemetry>();
    if state.enabled {
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
        let Some(action) = &event.action else {
            return;
        };
        // Resolved compile-time command names only. Never log typed keys,
        // text, input context values or clipboard contents.
        use glim_services::telemetry::{flag, text, track};
        let modifiers = event.keystroke.modifiers;
        let key = match event.keystroke.key.as_str() {
            "left" => "left",
            "right" => "right",
            "up" => "up",
            "down" => "down",
            "pageup" => "pageup",
            "pagedown" => "pagedown",
            "home" => "home",
            "end" => "end",
            "backspace" => "backspace",
            "delete" => "delete",
            "enter" => "enter",
            "escape" => "escape",
            "tab" => "tab",
            "space" => "space",
            "f1" => "f1",
            "," => "comma",
            "[" => "left_bracket",
            "]" => "right_bracket",
            "+" | "=" => "plus",
            "-" => "minus",
            "0" => "zero",
            "1" => "one",
            key if modifiers.platform || modifiers.control || modifiers.alt => match key {
                "a" => "a",
                "b" => "b",
                "c" => "c",
                "e" => "e",
                "f" => "f",
                "i" => "i",
                "n" => "n",
                "o" => "o",
                "p" => "p",
                "q" => "q",
                "r" => "r",
                "s" => "s",
                "t" => "t",
                "v" => "v",
                "w" => "w",
                "x" => "x",
                "z" => "z",
                _ => "other_bound_key",
            },
            _ => "other_bound_key",
        };
        let mut props = Vec::new();
        for (view, handle) in super::OpenWorkspaces::windows(cx) {
            if handle.window_id() == window.window_handle().window_id()
                && let Some(view) = view.upgrade()
            {
                props.extend(view.read(cx).usage_context());
                break;
            }
        }
        props.extend([
            ("action", text(action.name())),
            ("key_token", text(key)),
            ("command", flag(modifiers.platform)),
            ("control", flag(modifiers.control)),
            ("option", flag(modifiers.alt)),
            ("shift", flag(modifiers.shift)),
        ]);
        track("shortcut_used", props);
    })
    .detach();
}
