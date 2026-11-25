use glim_services::locale::tr;
use gpui_kit::{
    component::{
        ActiveTheme,
        input::{Input, InputEvent, InputState},
    },
    *,
};
use std::path::PathBuf;

pub(super) struct InlineRename {
    pub path: PathBuf,
    input: Entity<InputState>,
    _subscription: Subscription,
}

impl InlineRename {
    pub fn new<V: 'static>(
        path: PathBuf,
        is_directory: bool,
        window: &mut Window,
        cx: &mut Context<V>,
        finish: impl Fn(&mut V, bool, &mut Window, &mut Context<V>) + 'static,
    ) -> Self {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let selected = if is_directory {
            name.len()
        } else {
            path.file_stem().unwrap_or_default().to_string_lossy().len()
        };
        let input = cx.new(|cx| {
            let mut input = InputState::new(window, cx);
            input.set_value(name.into_owned(), window, cx);
            input.set_selected_range(0..selected, cx);
            input
        });
        let subscription =
            cx.subscribe_in(
                &input,
                window,
                move |view, _, event, window, cx| match event {
                    InputEvent::PressEnter { .. } => finish(view, true, window, cx),
                    InputEvent::Blur => finish(view, false, window, cx),
                    _ => {}
                },
            );
        input.update(cx, |input, cx| input.focus(window, cx));
        Self {
            path,
            input,
            _subscription: subscription,
        }
    }

    pub fn destination(&self, cx: &App) -> Result<Option<PathBuf>, String> {
        let value = self.input.read(cx).value();
        // Compare the displayed name first so an unchanged non-UTF-8 name is a no-op.
        if self
            .path
            .file_name()
            .is_some_and(|name| name.to_string_lossy() == value.as_ref())
        {
            return Ok(None);
        }
        if value.trim().is_empty()
            || value.contains(['/', '\0'])
            || matches!(value.as_ref(), "." | "..")
        {
            return Err(tr("Enter a filename without slashes"));
        }
        Ok(Some(self.path.with_file_name(value.as_ref())))
    }

    pub fn field(&self, height: Pixels, cx: &App) -> Input {
        Input::new(&self.input)
            .appearance(false)
            .border_1()
            .border_color(super::accent(cx))
            .rounded(px(4.))
            .bg(cx.theme().background)
            .h(height)
            .min_w_0()
            .px(px(2.))
            .py_0()
            .text_size(px(12.))
            .aria_label(tr("Rename…"))
    }
}
