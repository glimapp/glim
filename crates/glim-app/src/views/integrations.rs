use glim_services::{
    integrations::{self, Kind, Status, Target},
    locale::tr,
};
use gpui_kit::{
    assets::IconName,
    component::{
        ActiveTheme, Disableable, Icon, Sizable,
        button::Button,
        h_flex,
        menu::{DropdownMenu, PopupMenuItem},
        tooltip::Tooltip,
        v_flex,
    },
    prelude::FluentBuilder,
    *,
};

#[derive(Default)]
pub(crate) struct Integrations {
    targets: Vec<Target>,
    loaded: bool,
    busy: bool,
    pending_config: Option<std::path::PathBuf>,
    task: Option<Task<()>>,
    notice: Option<(&'static str, bool)>,
    notice_task: Option<Task<()>>,
}

impl Integrations {
    pub(crate) fn reload(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        let work = cx
            .background_executor()
            .spawn(async { integrations::discover() });
        self.task = Some(cx.spawn(async move |view, cx| {
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                view.loaded = true;
                match result {
                    Ok(targets) => view.targets = targets,
                    Err(_) => {
                        view.notice("Could not detect applications. Try refreshing.", true, cx)
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn notice(&mut self, message: &'static str, error: bool, cx: &mut Context<Self>) {
        self.notice = Some((message, error));
        self.notice_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(if error { 6 } else { 3 }))
                .await;
            let _ = view.update(cx, |view, cx| {
                view.notice = None;
                cx.notify();
            });
        }));
    }

    fn configure(&mut self, target: Target, install: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.pending_config = Some(target.config.clone());
        self.notice = None;
        self.notice_task = None;
        let work = cx
            .background_executor()
            .spawn(async move { integrations::configure(&target, install) });
        self.task = Some(cx.spawn(async move |view, cx| {
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                view.pending_config = None;
                if result.is_ok() {
                    view.notice("Restart the target app to apply.", false, cx);
                } else {
                    view.notice(
                        "Could not update configuration. Refresh and try again.",
                        true,
                        cx,
                    );
                }
                view.reload(cx);
            });
        }));
        cx.notify();
    }

    fn add_configuration(&mut self, kind: Kind, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some(format!("{} · {}", kind.label(), tr("Select configuration")).into()),
        });
        self.busy = true;
        self.task = Some(cx.spawn(async move |view, cx| {
            let paths = selection.await;
            let path = match paths {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let result = if let Some(path) = path {
                Some(
                    cx.background_executor()
                        .spawn(async move { integrations::remember_configuration(kind, &path) })
                        .await,
                )
            } else {
                None
            };
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                if result.is_some_and(|result| result.is_err()) {
                    view.notice("Select this app's configuration file or folder.", true, cx);
                }
                view.reload(cx);
            });
        }));
        cx.notify();
    }
}

impl Render for Integrations {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows = v_flex().w_full().px_4().rounded(px(12.)).bg(cx
            .theme()
            .background
            .blend(cx.theme().foreground.opacity(0.035)));
        for (index, target) in self.targets.iter().enumerate() {
            let configured = target.status == Status::Configured;
            let blocked = matches!(target.status, Status::Conflict | Status::Unreadable);
            let usage = tr(if target.kind == Kind::Codex {
                "Default file opener"
            } else {
                "Open files from chat"
            });
            let detail = if self
                .targets
                .iter()
                .filter(|other| other.kind == target.kind)
                .count()
                > 1
            {
                format!("{usage} · {}", target.detail)
            } else {
                usage
            };
            let config_path = target.config.display().to_string();
            let button = Button::new(("integration-action", index))
                .small()
                .h(px(30.))
                .min_w(px(90.))
                .border_1()
                .border_color(cx.theme().border)
                .rounded(px(8.))
                .disabled(self.busy || blocked)
                .loading(self.pending_config.as_ref() == Some(&target.config))
                .label(tr(if configured {
                    "Configured"
                } else if blocked {
                    "Unavailable"
                } else {
                    "Add integration"
                }));
            let control = if configured {
                let entity = cx.entity().downgrade();
                let target = target.clone();
                button
                    .icon(Icon::new(IconName::ChevronDown).size(px(12.)))
                    .dropdown_menu(move |menu, _, _| {
                        let menu = if target.kind == Kind::Codex {
                            let entity = entity.clone();
                            let target = target.clone();
                            menu.item(PopupMenuItem::new(tr("Set as default")).on_click(
                                move |_, _, cx| {
                                    let _ = entity.update(cx, |view, cx| {
                                        view.configure(target.clone(), true, cx)
                                    });
                                },
                            ))
                        } else {
                            menu
                        };
                        let entity = entity.clone();
                        let target = target.clone();
                        menu.item(PopupMenuItem::new(tr("Remove integration")).on_click(
                            move |_, _, cx| {
                                let _ = entity.update(cx, |view, cx| {
                                    view.configure(target.clone(), false, cx)
                                });
                            },
                        ))
                    })
                    .into_any_element()
            } else {
                let target = target.clone();
                button
                    .on_click(
                        cx.listener(move |view, _, _, cx| view.configure(target.clone(), true, cx)),
                    )
                    .into_any_element()
            };
            if index > 0 {
                rows = rows.child(div().h(px(1.)).w_full().bg(cx.theme().border));
            }
            let label = h_flex().gap_2().child(div().text_sm().child(target.kind.label()))
                .when(configured || blocked, |row| {
                    let status = tr(match target.status {
                        Status::Configured => "Configured",
                        Status::Conflict => "An existing Glim entry needs review. Open the configuration file.",
                        _ => "Cannot read this configuration. Open the file to check its format and permissions.",
                    });
                    row.child(div().id(("integration-status",index)).size(px(6.)).rounded_full()
                        .bg(if blocked { cx.theme().danger } else { super::accent(cx) })
                        .role(Role::Status).aria_label(status.clone())
                        .tooltip(move |window,cx| Tooltip::new(status.clone()).build(window,cx)))
                });
            rows = rows.child(
                h_flex()
                    .w_full()
                    .min_h(px(64.))
                    .py_3()
                    .gap_3()
                    .child(
                        v_flex().flex_1().min_w_0().gap_1().child(label).child(
                            div()
                                .id(("integration-path", index))
                                .text_xs()
                                .truncate()
                                .text_color(cx.theme().muted_foreground)
                                .tooltip(move |window, cx| {
                                    Tooltip::new(config_path.clone()).build(window, cx)
                                })
                                .child(detail),
                        ),
                    )
                    .child(control)
                    .when(blocked, |row| {
                        let path = target.config.clone();
                        row.child(
                            super::icon_button(
                                ("integration-reveal", index),
                                IconName::Folder,
                                tr("Show in Finder"),
                            )
                            .on_click(move |_, _, cx| cx.reveal_path(&path)),
                        )
                    }),
            );
        }
        if self.targets.is_empty() {
            rows = rows.child(
                h_flex()
                    .min_h(px(64.))
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .when(self.loaded, |row| {
                        row.child(tr("No supported applications detected."))
                    })
                    .when(!self.loaded, |row| {
                        row.child(super::loading_icon(
                            "integrations-loading",
                            px(16.),
                            tr("Detecting applications…"),
                            cx,
                        ))
                    }),
            );
        }
        let status = if let Some((message, error)) = self.notice {
            div()
                .id("integration-notice")
                .flex_1()
                .min_w_0()
                .truncate()
                .role(Role::Status)
                .tooltip(move |window, cx| Tooltip::new(tr(message)).build(window, cx))
                .text_xs()
                .text_color(if error {
                    cx.theme().danger
                } else {
                    cx.theme().muted_foreground
                })
                .child(tr(message))
                .into_any_element()
        } else {
            div().into_any_element()
        };
        v_flex()
            .w_full()
            .gap_3()
            .mt_3()
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr("App integrations")),
                    )
                    .child(
                        super::icon_button(
                            "integrations-refresh",
                            IconName::RotateCw,
                            tr("Refresh"),
                        )
                        .disabled(self.busy)
                        .loading(self.busy)
                        .on_click(cx.listener(|view, _, _, cx| view.reload(cx))),
                    ),
            )
            .child(rows)
            .child(
                h_flex()
                    .w_full()
                    .gap_3()
                    .h(px(32.))
                    .child(
                        Button::new("integration-add-configuration")
                            .small()
                            .border_1()
                            .border_color(cx.theme().border)
                            .rounded(px(8.))
                            .disabled(self.busy)
                            .label(tr("Add configuration…"))
                            .dropdown_menu({
                                let entity = cx.entity().downgrade();
                                move |mut menu, _, _| {
                                    for kind in Kind::ALL {
                                        let entity = entity.clone();
                                        menu =
                                            menu.item(PopupMenuItem::new(kind.label()).on_click(
                                                move |_, _, cx| {
                                                    let _ = entity.update(cx, |view, cx| {
                                                        view.add_configuration(kind, cx)
                                                    });
                                                },
                                            ));
                                    }
                                    menu
                                }
                            }),
                    )
                    .child(status),
            )
    }
}
