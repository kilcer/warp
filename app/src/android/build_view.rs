use std::sync::{Arc, Mutex};

use warpui::{
    TypedActionView,
    elements::{
        ClippedScrollStateHandle, ClippedScrollable, Container, CrossAxisAlignment, Element,
        Flex, MainAxisAlignment, MainAxisSize, ParentElement, ScrollbarWidth,
        SelectableArea, SelectionHandle, Shrinkable, Text,
    },
    AppContext, Entity, SingletonEntity, View, ViewContext,
};

use crate::appearance::Appearance;
use crate::android::build_state::BUILD_STATE;

/// Event emitted when the user interacts with the build view.
#[derive(Clone, Debug)]
pub enum BuildViewEvent {
    Clear,
}

/// A view that displays build output similar to Android Studio's Build tab.
pub struct BuildView {
    scroll_state: ClippedScrollStateHandle,
    selection_handle: SelectionHandle,
    /// Text selected by the user, staged for clipboard copy on next poll tick.
    pending_selection: Arc<Mutex<Option<String>>>,
}

impl BuildView {
    pub fn new() -> Self {
        Self {
            scroll_state: ClippedScrollStateHandle::new(),
            selection_handle: SelectionHandle::default(),
            pending_selection: Arc::new(Mutex::new(None)),
        }
    }

    /// Scrolls the output to the bottom and copies any pending selection to clipboard.
    /// Called from the poll callback which runs on the main thread with ViewContext.
    pub fn auto_scroll(&mut self, ctx: &mut ViewContext<Self>) {
        use warpui::units::Pixels;
        self.scroll_state.scroll_to(Pixels::new(f32::MAX));

        // Copy pending selection to clipboard (copy-on-select).
        if let Some(text) = self.pending_selection.lock().unwrap().take() {
            use warpui::clipboard::ClipboardContent;
            ctx.clipboard().write(ClipboardContent::plain_text(text));
        }
    }
}

impl Entity for BuildView {
    type Event = BuildViewEvent;
}

impl TypedActionView for BuildView {
    type Action = BuildViewEvent;

    fn handle_action(&mut self, _action: &Self::Action, _ctx: &mut ViewContext<Self>) {}
}

impl View for BuildView {
    fn ui_name() -> &'static str {
        "BuildView"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let bg = theme.background();
        let font_family = appearance.ui_font_family();
        let font_size = 11.0;

        // Clone data out of the lock so we don't borrow across render boundaries.
        let (running, success, lines) = {
            let state = BUILD_STATE.lock().unwrap();
            (
                state.running,
                state.success,
                state.lines.iter().map(|l| l.clone()).collect::<Vec<_>>(),
            )
        };

        if lines.is_empty() && !running {
            let hint = Text::new(
                "Build output will appear here. Use Run to start a build.",
                font_family,
                font_size,
            )
            .with_color(theme.disabled_text_color(bg).into())
            .finish();
            return Container::new(hint)
                .with_padding_top(8.)
                .with_padding_left(12.)
                .with_padding_bottom(8.)
                .finish();
        }

        let mut column = Flex::column()
            .with_main_axis_alignment(MainAxisAlignment::Start)
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
            .with_main_axis_size(MainAxisSize::Min);

        // Status line
        let status_text = if running {
            "Build running..."
        } else {
            match success {
                Some(true) => "Build finished: SUCCESS",
                Some(false) => "Build finished: FAILED",
                None => "",
            }
        };

        let status_color = if running {
            theme.main_text_color(bg)
        } else {
            match success {
                Some(true) => theme.main_text_color(bg),
                Some(false) => theme.disabled_text_color(bg),
                None => theme.disabled_text_color(bg),
            }
        };

        column.add_child(
            Text::new(status_text, font_family, 12.0)
                .with_color(status_color.into())
                .finish(),
        );

        // Build log lines
        let max_lines = 200;
        let start = if lines.len() > max_lines {
            lines.len() - max_lines
        } else {
            0
        };

        for line in &lines[start..] {
            column.add_child(
                Text::new(line.clone(), font_family, 10.0)
                    .with_color(theme.sub_text_color(bg).into())
                    .finish(),
            );
        }

        let content = Container::new(column.finish())
            .with_padding_top(4.)
            .with_padding_left(12.)
            .with_padding_right(12.)
            .with_padding_bottom(4.)
            .finish();

        // Selection handler: store selected text for copy-on-select.
        let pending = self.pending_selection.clone();
        let selectable = SelectableArea::new(
            self.selection_handle.clone(),
            move |args, _ctx, _app| {
                if let Some(ref text) = args.selection {
                    *pending.lock().unwrap() = Some(text.clone());
                }
            },
            content,
        )
        .finish();

        let scrollable = ClippedScrollable::vertical(
            self.scroll_state.clone(),
            selectable,
            ScrollbarWidth::Auto,
            theme.disabled_text_color(bg).into(),
            theme.main_text_color(bg).into(),
            theme.surface_2().into(),
        )
        .finish();

        Shrinkable::new(1., scrollable).finish()
    }
}
