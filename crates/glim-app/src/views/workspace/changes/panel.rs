use super::*;
use gpui_kit::assets::IconName;

impl Render for Changes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !git::enabled() {
            return message(tr("Git is unavailable in this build"), cx);
        }
        let sections = [(Section::Changes, "Changes"), (Section::History, "History")];
        let mut panel = v_flex()
            .size_full()
            .min_h_0()
            .gap_1()
            .child(self.repository_header(cx))
            .child(
                h_flex()
                    .px_2()
                    .gap_1()
                    .children(sections.map(|(section, label)| {
                        Button::new(("git-section", section as usize))
                            .small()
                            .ghost()
                            .label(tr(label))
                            .selected(self.section == section)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.switch_section(section, cx);
                                cx.emit(ShowDiff);
                            }))
                    })),
            );
        if let Some(error) = &self.fetch_error {
            let error = error.clone();
            panel = panel.child(
                super::controls::icon_action(
                    Button::new("git-fetch-error"),
                    IconName::Copy,
                    "Copy error details",
                )
                .label(tr("Git operation failed"))
                .tooltip(error.clone())
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(error.to_string()))
                }),
            );
        }
        if let Some(operation) = self
            .repositories
            .first()
            .and_then(|repository| repository.metadata.operation.as_deref())
        {
            let conflicts = self
                .rows
                .iter()
                .filter(|changes| changes.iter().any(|change| change.status == 'U'))
                .count();
            panel = panel.child(
                div()
                    .px_2()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{operation} · {} {conflicts}", tr("Conflicts"))),
            );
        }
        let content = if self.section == Section::History {
            self.history_view(window, cx)
        } else {
            let count = self.active_change_count();
            v_flex()
                .flex_1()
                .min_h_0()
                .child(
                    h_flex()
                        .px_2()
                        .gap_1()
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("{} · {count}", tr("Changes"))),
                        )
                        .child(
                            super::controls::icon_action(
                                Button::new("all-changes-diff"),
                                IconName::Layers,
                                "All changes",
                            )
                            .disabled(count == 0)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.open_comparison(Comparison::AllChanges, cx)
                            })),
                        ),
                )
                .child(self.navigation(cx))
                .into_any_element()
        };
        panel.child(content).into_any_element()
    }
}
