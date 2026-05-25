use std::sync::{Arc, Mutex};

use warpui::{
    TypedActionView,
    elements::{
        ClippedScrollStateHandle, ClippedScrollable, Container, CrossAxisAlignment, Element,
        Flex, Hoverable, MainAxisAlignment, MainAxisSize, MouseStateHandle, ParentElement,
        ScrollbarWidth, SelectableArea, SelectionHandle, Shrinkable, Text,
    },
    AppContext, Entity, SingletonEntity, View, ViewContext, ViewHandle,
};

use crate::appearance::Appearance;
use crate::android::logcat_state::LOGCAT_STATE;
use crate::editor::{
    EditorView, Event as EditorEvent,
    SingleLineEditorOptions, TextOptions,
};

/// Log level labels for the filter buttons.
const LEVEL_LABELS: [&str; 6] = ["V", "D", "I", "W", "E", "F"];

/// Event emitted when the user interacts with the logcat view.
#[derive(Clone, Debug)]
pub enum LogcatViewEvent {
    Clear,
    SelectDevice(String),
    SetLogLevel(u8),
    TogglePackageFilter,
    ToggleFollow,
}

/// A view that displays logcat output with filtering.
pub struct LogcatView {
    scroll_state: ClippedScrollStateHandle,
    selection_handle: SelectionHandle,
    pending_selection: Arc<Mutex<Option<String>>>,
    device_label_mouse_state: MouseStateHandle,
    /// Minimum log level to display (0=V, 1=D, 2=I, 3=W, 4=E, 5=F).
    min_log_level: u8,
    /// Text filter — only lines containing this string (case-insensitive) are shown.
    filter_text: String,
    /// Editor view handle for the text filter input.
    filter_editor: ViewHandle<EditorView>,
    /// Mouse state handles for the 6 level filter buttons.
    level_mouse_states: Vec<MouseStateHandle>,
    /// Whether the package PID filter is active.
    use_package_filter: bool,
    /// Mouse state for the package filter toggle button.
    pkg_filter_mouse_state: MouseStateHandle,
    /// Whether to auto-scroll to bottom on new output.
    follow_bottom: bool,
    /// Mouse state for the follow toggle button.
    follow_mouse_state: MouseStateHandle,
    /// Mouse state for right-click detection on the log area.
    right_click_mouse_state: MouseStateHandle,
}

impl LogcatView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let filter_editor = ctx.add_typed_action_view(|ctx| {
            let appearance = Appearance::as_ref(ctx);
            let mut editor = EditorView::single_line(
                SingleLineEditorOptions {
                    text: TextOptions::ui_text(Some(11.0), appearance),
                    ..Default::default()
                },
                ctx,
            );
            editor.set_placeholder_text("Filter...", ctx);
            editor
        });

        ctx.subscribe_to_view(&filter_editor, |me, _, event, ctx| {
            if let EditorEvent::Edited(_) = event {
                let text = me.filter_editor.read(ctx, |editor, ctx| editor.buffer_text(ctx));
                me.filter_text = text;
                ctx.notify();
            }
        });

        let level_mouse_states = (0..6)
            .map(|_| MouseStateHandle::default())
            .collect();

        Self {
            scroll_state: ClippedScrollStateHandle::new(),
            selection_handle: SelectionHandle::default(),
            pending_selection: Arc::new(Mutex::new(None)),
            device_label_mouse_state: MouseStateHandle::default(),
            min_log_level: 0,
            filter_text: String::new(),
            filter_editor,
            level_mouse_states,
            use_package_filter: true,
            pkg_filter_mouse_state: MouseStateHandle::default(),
            follow_bottom: true,
            follow_mouse_state: MouseStateHandle::default(),
            right_click_mouse_state: MouseStateHandle::default(),
        }
    }

    /// Scrolls to bottom (if following) and copies pending selection. Called from poll callback.
    pub fn auto_scroll(&mut self, ctx: &mut ViewContext<Self>) {
        use warpui::units::Pixels;

        if self.follow_bottom {
            self.scroll_state.scroll_to(Pixels::new(f32::MAX));
        }

        if let Some(text) = self.pending_selection.lock().unwrap().take() {
            use warpui::clipboard::ClipboardContent;
            ctx.clipboard().write(ClipboardContent::plain_text(text));
        }
    }
}

impl Entity for LogcatView {
    type Event = LogcatViewEvent;
}

impl TypedActionView for LogcatView {
    type Action = LogcatViewEvent;

    fn handle_action(&mut self, action: &Self::Action, _ctx: &mut ViewContext<Self>) {
        match action {
            LogcatViewEvent::Clear => {
                if let Ok(mut state) = LOGCAT_STATE.lock() {
                    state.entries.clear();
                }
            }
            LogcatViewEvent::SelectDevice(serial) => {
                if let Ok(mut state) = LOGCAT_STATE.lock() {
                    state.selected_serial = Some(serial.clone());
                }
            }
            LogcatViewEvent::SetLogLevel(level) => {
                self.min_log_level = *level;
            }
            LogcatViewEvent::TogglePackageFilter => {
                self.use_package_filter = !self.use_package_filter;
            }
            LogcatViewEvent::ToggleFollow => {
                self.follow_bottom = !self.follow_bottom;
            }
        }
    }
}

/// Returns true if the line passes both the level and text filters.
fn line_matches_filter(line: &str, filter_text: &str, min_level: u8) -> bool {
    let level_char = line
        .split_whitespace()
        .nth(4)
        .and_then(|s| s.chars().next());
    let level_priority = match level_char {
        Some('V') => 0,
        Some('D') => 1,
        Some('I') => 2,
        Some('W') => 3,
        Some('E') => 4,
        Some('F') => 5,
        _ => 0,
    };
    if level_priority < min_level {
        return false;
    }
    if !filter_text.is_empty()
        && !line.to_lowercase().contains(&filter_text.to_lowercase())
    {
        return false;
    }
    true
}

impl View for LogcatView {
    fn ui_name() -> &'static str {
        "LogcatView"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let bg = theme.background();
        let font_family = appearance.ui_font_family();
        let font_size = 10.0;

        let (running, lines, devices, selected_serial, package_name) = {
            let state = LOGCAT_STATE.lock().unwrap();
            (
                state.running,
                state.entries.iter().map(|l| l.clone()).collect::<Vec<_>>(),
                state.devices.clone(),
                state.selected_serial.clone(),
                state.package_name.clone(),
            )
        };

        // Build effective filter text: if package filter is on, use package name as filter.
        let effective_filter = if self.use_package_filter {
            package_name.clone().unwrap_or_default()
        } else {
            self.filter_text.clone()
        };

        // Apply filters.
        let filtered: Vec<String> = lines
            .iter()
            .filter(|l| line_matches_filter(l, &effective_filter, self.min_log_level))
            .cloned()
            .collect();

        let mut root = Flex::column()
            .with_main_axis_alignment(MainAxisAlignment::Start)
            .with_cross_axis_alignment(CrossAxisAlignment::Start);

        // --- Device selector toolbar ---
        root.add_child(Self::render_device_toolbar(
            appearance,
            &devices,
            &selected_serial,
            &self.device_label_mouse_state,
        ));

        // --- Filter toolbar ---
        root.add_child(Self::render_filter_toolbar(
            appearance,
            self.min_log_level,
            &self.level_mouse_states,
            &self.filter_editor,
            &package_name,
            self.use_package_filter,
            &self.pkg_filter_mouse_state,
            self.follow_bottom,
            &self.follow_mouse_state,
        ));

        // --- Logcat output ---
        if filtered.is_empty() && !running {
            let hint = if lines.is_empty() {
                "Select a device above to start logcat streaming."
            } else {
                "No lines match the current filter."
            };
            let hint_el = Text::new(hint, font_family, font_size)
                .with_color(theme.disabled_text_color(bg).into())
                .finish();
            root.add_child(
                Container::new(hint_el)
                    .with_padding_top(8.)
                    .with_padding_left(12.)
                    .finish(),
            );
        } else {
            // Status line
            let status_text = if running {
                "Logcat streaming..."
            } else {
                "Logcat stopped"
            };
            let status_color = theme.main_text_color(bg);

            let mut column = Flex::column()
                .with_main_axis_alignment(MainAxisAlignment::Start)
                .with_cross_axis_alignment(CrossAxisAlignment::Start)
                .with_main_axis_size(MainAxisSize::Min);

            column.add_child(
                Text::new(status_text, font_family, 11.0)
                    .with_color(status_color.into())
                    .finish(),
            );

            let max_lines = 200;
            let start = if filtered.len() > max_lines {
                filtered.len() - max_lines
            } else {
                0
            };

            for line in &filtered[start..] {
                let color = logcat_line_color(line, theme, bg);
                column.add_child(
                    Text::new(line.clone(), font_family, font_size)
                        .with_color(color.into())
                        .finish(),
                );
            }

            let content = Container::new(column.finish())
                .with_padding_top(4.)
                .with_padding_left(12.)
                .with_padding_right(12.)
                .with_padding_bottom(4.)
                .finish();

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

            // Wrap in Hoverable for right-click to clear.
            let hoverable = Hoverable::new(self.right_click_mouse_state.clone(), |_| selectable)
                .on_right_click(|ctx, _, _| {
                    ctx.dispatch_typed_action(LogcatViewEvent::Clear);
                })
                .finish();

            let scrollable = ClippedScrollable::vertical(
                self.scroll_state.clone(),
                hoverable,
                ScrollbarWidth::Auto,
                theme.disabled_text_color(bg).into(),
                theme.main_text_color(bg).into(),
                theme.surface_2().into(),
            )
            .finish();

            root.add_child(Shrinkable::new(1., scrollable).finish());
        }

        root.finish()
    }
}

impl LogcatView {
    #[allow(clippy::too_many_arguments)]
    fn render_filter_toolbar(
        appearance: &Appearance,
        min_level: u8,
        mouse_states: &[MouseStateHandle],
        filter_editor: &ViewHandle<EditorView>,
        package_name: &Option<String>,
        use_package_filter: bool,
        pkg_filter_mouse_state: &MouseStateHandle,
        follow_bottom: bool,
        follow_mouse_state: &MouseStateHandle,
    ) -> Box<dyn Element> {
        use warpui::platform::Cursor;
        use warpui::ui_components::components::{UiComponentStyles, UiComponent, Coords};
        use warpui::ui_components::button::ButtonVariant;

        let theme = appearance.theme();
        let bg = theme.background();

        let mut row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        // Level filter buttons.
        for (i, label) in LEVEL_LABELS.iter().enumerate() {
            let is_selected = i as u8 == min_level;
            let is_active = i as u8 >= min_level;

            let label_color = if is_selected {
                theme.accent().into_solid()
            } else if is_active {
                theme.main_text_color(bg).into_solid()
            } else {
                theme.disabled_text_color(bg).into_solid()
            };

            let level = i as u8;
            let btn = appearance
                .ui_builder()
                .button(ButtonVariant::Text, mouse_states[i].clone())
                .with_style(UiComponentStyles {
                    font_size: Some(11.),
                    padding: Some(Coords::default()
                        .left(4.).right(4.).top(1.).bottom(1.)),
                    font_color: Some(label_color.into()),
                    ..Default::default()
                })
                .with_text_label(label.to_string())
                .build()
                .on_click(move |ctx: &mut warpui::EventContext, _, _| {
                    ctx.dispatch_typed_action(LogcatViewEvent::SetLogLevel(level));
                })
                .with_cursor(Cursor::PointingHand)
                .finish();

            row = row.with_child(btn);
        }

        // Package filter toggle (only if package name is known).
        if let Some(pkg) = package_name {
            let pkg_label = if pkg.len() > 20 {
                format!("{}...", &pkg[..17])
            } else {
                pkg.clone()
            };
            let display = format!("App: {}", pkg_label);

            let chip_color = if use_package_filter {
                theme.accent().into_solid()
            } else {
                theme.disabled_text_color(bg).into_solid()
            };
            let chip_bg = if use_package_filter {
                theme.accent().with_opacity(15).into()
            } else {
                warpui::elements::Fill::Solid(pathfinder_color::ColorU::transparent_black())
            };

            let chip = appearance
                .ui_builder()
                .button(ButtonVariant::Text, pkg_filter_mouse_state.clone())
                .with_style(UiComponentStyles {
                    font_size: Some(10.),
                    padding: Some(Coords::default()
                        .left(6.).right(6.).top(1.).bottom(1.)),
                    font_color: Some(chip_color.into()),
                    background: Some(chip_bg),
                    border_radius: Some(warpui::elements::CornerRadius::with_all(
                        warpui::elements::Radius::Pixels(3.0)
                    )),
                    ..Default::default()
                })
                .with_text_label(display)
                .build()
                .on_click(|ctx: &mut warpui::EventContext, _, _| {
                    ctx.dispatch_typed_action(LogcatViewEvent::TogglePackageFilter);
                })
                .with_cursor(Cursor::PointingHand)
                .finish();

            row = row.with_child(chip);
        }

        // Follow toggle button.
        {
            let follow_label = if follow_bottom { "Follow" } else { "Paused" };
            let follow_color = if follow_bottom {
                theme.accent().into_solid()
            } else {
                theme.disabled_text_color(bg).into_solid()
            };

            let follow_btn = appearance
                .ui_builder()
                .button(ButtonVariant::Text, follow_mouse_state.clone())
                .with_style(UiComponentStyles {
                    font_size: Some(10.),
                    padding: Some(Coords::default()
                        .left(4.).right(4.).top(1.).bottom(1.)),
                    font_color: Some(follow_color.into()),
                    ..Default::default()
                })
                .with_text_label(follow_label.to_string())
                .build()
                .on_click(|ctx: &mut warpui::EventContext, _, _| {
                    ctx.dispatch_typed_action(LogcatViewEvent::ToggleFollow);
                })
                .with_cursor(Cursor::PointingHand)
                .finish();

            row = row.with_child(follow_btn);
        }

        // Separator spacing.
        let separator = Text::new(" ", appearance.ui_font_family(), 11.0)
            .finish();
        row = row.with_child(separator);

        // Text filter input.
        let input = appearance
            .ui_builder()
            .text_input(filter_editor.clone())
            .with_style(UiComponentStyles {
                padding: Some(Coords::default().left(6.).right(6.).top(2.).bottom(2.)),
                font_size: Some(11.),
                ..Default::default()
            })
            .build()
            .finish();

        row = row.with_child(Shrinkable::new(1., input).finish());

        Container::new(row.with_main_axis_size(MainAxisSize::Max).finish())
            .with_padding_left(8.)
            .with_padding_top(2.)
            .with_padding_bottom(2.)
            .with_border(warpui::elements::Border::bottom(1.0)
                .with_border_fill(theme.outline()))
            .finish()
    }

    fn render_device_toolbar(
        appearance: &Appearance,
        devices: &[crate::android::device::AndroidDevice],
        selected_serial: &Option<String>,
        mouse_state: &MouseStateHandle,
    ) -> Box<dyn Element> {
        use warpui::platform::Cursor;
        use warpui::ui_components::components::{UiComponentStyles, UiComponent};
        use warpui::ui_components::button::ButtonVariant;

        let theme = appearance.theme();
        let bg = theme.background();

        // Find current device label and next serial for cycling.
        let (label, next_serial) = if devices.is_empty() {
            ("No device".to_string(), None)
        } else {
            let idx = if let Some(ref serial) = selected_serial {
                devices.iter().position(|d| &d.serial == serial).unwrap_or(0)
            } else {
                0
            };
            let d = &devices[idx];
            let name = match (&d.model, &d.product) {
                (Some(m), _) if !m.is_empty() => m.clone(),
                (_, Some(p)) if !p.is_empty() => p.clone(),
                _ => d.serial.clone(),
            };
            let label = if devices.len() > 1 {
                format!("{} ({}/{})", name, idx + 1, devices.len())
            } else {
                name
            };
            let next_idx = (idx + 1) % devices.len();
            let next_serial = devices[next_idx].serial.clone();
            (label, Some(next_serial))
        };

        let has_devices = !devices.is_empty();
        let label_color = if has_devices {
            theme.main_text_color(bg)
        } else {
            theme.disabled_text_color(bg)
        };

        let next_serial_for_closure = next_serial.unwrap_or_default();
        let device_btn = appearance
            .ui_builder()
            .button(ButtonVariant::Text, mouse_state.clone())
            .with_style(UiComponentStyles {
                font_size: Some(11.),
                padding: Some(warpui::ui_components::components::Coords::default()
                    .left(6.).right(6.).top(2.).bottom(2.)),
                font_color: Some(label_color.into()),
                ..Default::default()
            })
            .with_text_label(label)
            .build()
            .on_click(move |ctx: &mut warpui::EventContext, _, _| {
                if !next_serial_for_closure.is_empty() {
                    ctx.dispatch_typed_action(LogcatViewEvent::SelectDevice(
                        next_serial_for_closure.clone(),
                    ));
                }
            })
            .with_cursor(Cursor::PointingHand)
            .finish();

        Container::new(
            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(device_btn)
                .with_main_axis_size(MainAxisSize::Max)
                .finish(),
        )
        .with_padding_left(8.)
        .with_padding_top(4.)
        .with_padding_bottom(2.)
        .with_border(warpui::elements::Border::bottom(1.0)
            .with_border_fill(theme.outline()))
        .finish()
    }
}

/// Determines a display color for a logcat line based on its log level.
/// Threadtime format: `MM-DD HH:MM:SS.mmm  PID  TID  L TAG: message`
fn logcat_line_color(
    line: &str,
    theme: &warp_core::ui::theme::WarpTheme,
    bg: warp_core::ui::theme::Fill,
) -> pathfinder_color::ColorU {
    let level_char = line
        .split_whitespace()
        .nth(4)
        .and_then(|s| s.chars().next());

    match level_char {
        Some('V') => theme.disabled_text_color(bg).into_solid(),
        Some('D') => theme.sub_text_color(bg).into_solid(),
        Some('I') => theme.main_text_color(bg).into_solid(),
        Some('W') => theme.terminal_colors().bright.yellow.into(),
        Some('E') | Some('F') => theme.terminal_colors().bright.red.into(),
        _ => theme.sub_text_color(bg).into_solid(),
    }
}
