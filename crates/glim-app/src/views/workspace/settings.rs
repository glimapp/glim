use super::Workspace;
use crate::app::actions::{CheckForUpdates, DefaultApplications, KeyboardShortcuts};
use glim_services::preferences::Appearance;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        ActiveTheme, Disableable, Icon, Sizable,
        button::Button,
        h_flex,
        menu::{DropdownMenu, PopupMenuItem},
        switch::Switch,
        v_flex,
    },
    *,
};

#[derive(Clone, Copy)]
enum PreferenceChoice {
    Theme(Appearance),
    Language(&'static str),
}

pub(super) fn prompt_telemetry_consent(window: &mut Window, cx: &mut App) {
    use crate::app::telemetry::{self, Telemetry};
    use glim_services::{locale::tr, telemetry::Consent};
    if !cx
        .try_global::<Telemetry>()
        .is_some_and(|state| state.needs_consent)
    {
        return;
    }
    cx.global_mut::<Telemetry>().needs_consent = false;
    let response = window.prompt(
        PromptLevel::Info,
        &tr("Help improve Glim"),
        Some(&tr("Error diagnostics help the developer fix failures and crashes. Usage statistics measure active installations, formats and features using a random installation ID that links sessions. Neither includes document contents or paths. Both are optional and can be changed independently in Settings.")),
        &[
            PromptButton::ok(tr("Error diagnostics only")),
            PromptButton::new(tr("Enable both")),
            PromptButton::cancel(tr("Not now")),
        ],
        cx,
    );
    cx.global_mut::<Telemetry>().consent_prompt = Some(cx.spawn(async move |cx| {
        let choice = response.await;
        cx.update(|cx| {
            if let Ok(choice) = choice {
                telemetry::set_consent(
                    Consent {
                        usage: choice == 1,
                        diagnostics: choice == 0 || choice == 1,
                    },
                    cx,
                );
            } else {
                cx.global_mut::<Telemetry>().needs_consent = true;
            }
        });
    }));
}

fn preference_picker(
    id: &'static str,
    label: &'static str,
    value: &'static str,
    options: Vec<(PreferenceChoice, &'static str)>,
    cx: &App,
) -> AnyElement {
    Button::new(id)
        .small()
        .min_w(px(110.))
        .h(px(30.))
        .border_1()
        .border_color(cx.theme().border)
        .rounded(px(8.))
        .disabled(crate::app::updates::busy(cx))
        .accessibility_label(format!(
            "{}: {}",
            glim_services::locale::tr(label),
            glim_services::locale::tr(value)
        ))
        .child(
            h_flex()
                .gap_2()
                .child(glim_services::locale::tr(value))
                .child(Icon::new(gpui_kit::assets::IconName::ChevronDown).size(px(14.))),
        )
        .dropdown_menu(move |mut menu, _, _| {
            for (choice, label) in &options {
                let choice = *choice;
                menu = menu.item(
                    PopupMenuItem::new(glim_services::locale::tr(label)).on_click(
                        move |_, _, cx| match choice {
                            PreferenceChoice::Theme(mode) => crate::app::set_appearance(mode, cx),
                            PreferenceChoice::Language(language) => {
                                crate::app::change_language(language, cx)
                            }
                        },
                    ),
                );
            }
            menu
        })
        .into_any_element()
}

impl Workspace {
    pub(super) fn settings_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let appearance = cx.global::<crate::app::AppearancePreference>().0;
        let theme = preference_picker(
            "settings-theme",
            "Theme",
            match appearance {
                Appearance::System => "Follow System",
                Appearance::Light => "Light",
                Appearance::Dark => "Dark",
            },
            vec![
                (PreferenceChoice::Theme(Appearance::System), "Follow System"),
                (PreferenceChoice::Theme(Appearance::Light), "Light"),
                (PreferenceChoice::Theme(Appearance::Dark), "Dark"),
            ],
            cx,
        );
        let languages = glim_services::locale::available();
        let current = cx.global::<crate::app::LanguagePreference>().0.as_str();
        let label = languages
            .iter()
            .find(|(code, _)| *code == current)
            .map_or("Follow System", |(_, name)| *name);
        let mut options = vec![(PreferenceChoice::Language("system"), "Follow System")];
        options.extend(
            languages
                .into_iter()
                .map(|(code, name)| (PreferenceChoice::Language(code), name)),
        );
        let language = preference_picker("settings-language", "Language", label, options, cx);
        let action_button = |id: &'static str, label: &'static str| {
            Button::new(id)
                .small()
                .min_w(px(110.))
                .h(px(30.))
                .border_1()
                .border_color(cx.theme().border)
                .rounded(px(8.))
                .disabled(crate::app::updates::busy(cx))
                .label(glim_services::locale::tr(label))
        };
        let associations = action_button("settings-file-types", "Manage…")
            .accessibility_label(glim_services::locale::tr("Default File Types…"))
            .on_click(|_, window, cx| window.dispatch_action(Box::new(DefaultApplications), cx))
            .into_any_element();
        let finder = action_button("settings-finder", "Manage…")
            .accessibility_label(glim_services::locale::tr("Finder Integration"))
            .on_click(|_, _, cx| crate::app::finder_integration::manage(cx))
            .into_any_element();
        let connections = action_button("settings-ssh", "Manage…")
            .accessibility_label(glim_services::locale::tr("SSH Connections"))
            .on_click(
                cx.listener(|view, _, window, cx| view.show_remote(None, true, false, window, cx)),
            )
            .into_any_element();
        let updates = action_button("settings-updates", crate::app::updates::check_label(cx))
            .disabled(crate::app::updates::busy(cx))
            .accessibility_label(glim_services::locale::tr("Check for Updates…"))
            .on_click(|_, window, cx| window.dispatch_action(Box::new(CheckForUpdates), cx))
            .into_any_element();
        let shortcuts = action_button("settings-shortcuts", "View…")
            .accessibility_label(glim_services::locale::tr("Keyboard Shortcuts…"))
            .on_click(|_, window, cx| window.dispatch_action(Box::new(KeyboardShortcuts), cx))
            .into_any_element();
        let consent = cx.global::<crate::app::telemetry::Telemetry>().consent;
        let usage = Switch::new("settings-usage-statistics")
            .small()
            .checked(consent.usage)
            .accessibility_label(glim_services::locale::tr("Usage statistics"))
            .on_click(|enabled, _, cx| crate::app::telemetry::set_usage(*enabled, cx))
            .into_any_element();
        let diagnostics = Switch::new("settings-error-diagnostics")
            .small()
            .checked(consent.diagnostics)
            .accessibility_label(glim_services::locale::tr("Error diagnostics"))
            .on_click(|enabled, _, cx| crate::app::telemetry::set_diagnostics(*enabled, cx))
            .into_any_element();
        let row = |label: &'static str, detail: &'static str, control: AnyElement| {
            let mut text = v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap_1()
                .child(div().text_sm().child(glim_services::locale::tr(label)));
            if !detail.is_empty() {
                text = text.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(glim_services::locale::tr(detail)),
                );
            }
            h_flex()
                .w_full()
                .min_h(px(58.))
                .py_3()
                .gap_4()
                .child(text)
                .child(div().flex_shrink_0().child(control))
        };
        let group = || {
            v_flex().w_full().px_4().rounded(px(12.)).bg(cx
                .theme()
                .background
                .blend(cx.theme().foreground.opacity(0.035)))
        };
        let divider = || div().w_full().h(px(1.)).bg(cx.theme().border);
        let heading = |label: &'static str| {
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(glim_services::locale::tr(label))
        };
        let information = |label: &'static str, value: String| {
            h_flex()
                .w_full()
                .min_h(px(36.))
                .gap_4()
                .text_sm()
                .child(
                    div()
                        .flex_1()
                        .text_color(cx.theme().muted_foreground)
                        .child(glim_services::locale::tr(label)),
                )
                .child(div().child(value))
        };
        let local_build = env!("GLIM_BUILD_CHANNEL") == "local";
        let version = if local_build {
            format!(
                "{} · {}",
                env!("CARGO_PKG_VERSION"),
                glim_services::locale::tr("Local build")
            )
        } else {
            env!("CARGO_PKG_VERSION").to_owned()
        };
        let mut about = group().py_2().child(information("Version", version));
        if local_build {
            about = about.child(information(
                "Built",
                env!("GLIM_BUILD_TIMESTAMP").to_owned(),
            ));
            if !env!("GLIM_BUILD_HASH").is_empty() {
                let mut revision = env!("GLIM_BUILD_HASH").to_owned();
                if env!("GLIM_BUILD_DIRTY") == "true" {
                    revision.push_str(" · ");
                    revision.push_str(&glim_services::locale::tr("Modified"));
                }
                about = about.child(information("Commit", revision));
            }
        }
        let telemetry = cx.global::<crate::app::telemetry::Telemetry>();
        let privacy_status = h_flex()
            .w_full()
            .min_h(px(36.))
            .gap_2()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .when(telemetry.persistence_error, |row| {
                row.child(div().flex_1().min_w(px(0.)).child(glim_services::locale::tr(
                    "Could not save privacy settings. Your choices apply only until Glim quits."
                ))).child(Button::new("retry-privacy-save")
                    .small()
                    .label(glim_services::locale::tr("Retry"))
                    .on_click(|_, _, cx| crate::app::telemetry::persist(cx)))
            })
            .when(telemetry.saving, |row| {
                row.child(glim_services::locale::tr("Saving…"))
            });
        div()
            .id("workspace-settings-scroll")
            .size_full()
            .overflow_y_scroll()
            .child(
                v_flex().w_full().items_center().px_6().py_6().child(
                    v_flex()
                        .w_full()
                        .max_w(px(560.))
                        .gap_3()
                        .child(heading("General"))
                        .child(
                            group()
                                .child(row("Theme", "", theme))
                                .child(divider())
                                .child(row("Language", "", language)),
                        )
                        .child(heading("Files").mt_3())
                        .child(group().child(row(
                            "Default File Types…",
                            "Choose which file types open in Glim.",
                            associations,
                        )).child(divider()).child(row(
                            "Finder Integration",
                            "Open folders as workspaces from the Finder context menu.",
                            finder,
                        )).child(divider()).child(row("SSH Connections", "Browse remote files in a read-only workspace.", connections)))
                        .child(heading("Application").mt_3())
                        .child(
                            group()
                                .child(row("Check for Updates…", "", updates))
                                .child(divider())
                                .child(row("Keyboard Shortcuts…", "", shortcuts)),
                        )
                        .child(heading("Help improve Glim").mt_3())
                        .child(group()
                            .child(row("Error diagnostics", "Help the developer find failures and fix crashes. Sends sanitized error types, application source locations and version information.", diagnostics))
                            .child(divider())
                            .child(row("Usage statistics", "Help measure active installations and improve frequently used formats and features. A random installation ID links usage across sessions.", usage)))
                        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(glim_services::locale::tr(
                            "Each option is independent. Turning both off stops usage and diagnostic reports. Document contents, filenames and paths are never included."
                        )))
                        .child(h_flex().child(Button::new("settings-privacy")
                            .small()
                            .label(glim_services::locale::tr("Privacy details"))
                            .on_click(|_, _, cx| cx.open_url(if glim_services::locale::code() == "zh-CN" {
                                "https://github.com/glimapp/glim/blob/main/docs/privacy.zh-CN.md"
                            } else {
                                "https://github.com/glimapp/glim/blob/main/docs/privacy.md"
                            }))))
                        .child(privacy_status)
                        .child(self.integrations.clone())
                        .child(heading("About").mt_3())
                        .child(about),
                ),
            )
            .into_any_element()
    }
}
