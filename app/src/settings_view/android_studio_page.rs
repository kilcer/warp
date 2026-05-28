use super::settings_page::{
    render_body_item, LocalOnlyIconState, MatchData, PageType,
    SettingsPageMeta, SettingsWidget, ToggleState,
};
use super::SettingsSection;
use crate::appearance::Appearance;
use warp_core::features::FeatureFlag;
use warpui::elements::Element;
use warpui::ui_components::components::UiComponent;
use warpui::ui_components::switch::SwitchStateHandle;
use warpui::{
    AppContext, Entity, TypedActionView, View, ViewContext,
};

// ---------------------------------------------------------------------------
// Actions & Events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum AndroidStudioPageAction {
    Toggle,
}

pub enum AndroidStudioPageEvent {}

// ---------------------------------------------------------------------------
// Page View
// ---------------------------------------------------------------------------

pub struct AndroidStudioPageView {
    page: PageType<Self>,
}

impl AndroidStudioPageView {
    pub fn new(_ctx: &mut ViewContext<Self>) -> Self {
        Self {
            page: PageType::new_uncategorized(
                vec![
                    Box::new(AndroidStudioModeToggle::default()),
                ],
                None,
            ),
        }
    }
}

impl Entity for AndroidStudioPageView {
    type Event = AndroidStudioPageEvent;
}

impl TypedActionView for AndroidStudioPageView {
    type Action = AndroidStudioPageAction;

    fn handle_action(&mut self, _action: &Self::Action, ctx: &mut ViewContext<Self>) {
        ctx.notify();
    }
}

impl View for AndroidStudioPageView {
    fn ui_name() -> &'static str {
        "AndroidStudioPage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl SettingsPageMeta for AndroidStudioPageView {
    fn section() -> SettingsSection {
        SettingsSection::AndroidStudio
    }

    fn should_render(&self, _ctx: &AppContext) -> bool {
        FeatureFlag::AndroidStudioMode.is_enabled()
    }

    fn update_filter(&mut self, query: &str, ctx: &mut ViewContext<Self>) -> MatchData {
        self.page.update_filter(query, ctx)
    }

    fn scroll_to_widget(&mut self, widget_id: &'static str) {
        self.page.scroll_to_widget(widget_id);
    }

    fn clear_highlighted_widget(&mut self) {
        self.page.clear_highlighted_widget();
    }
}

// ---------------------------------------------------------------------------
// Toggle Widget
// ---------------------------------------------------------------------------

#[derive(Default)]
struct AndroidStudioModeToggle {
    switch_state: SwitchStateHandle,
}

impl SettingsWidget for AndroidStudioModeToggle {
    type View = AndroidStudioPageView;

    fn search_terms(&self) -> &str {
        "android studio mode"
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        let ui_builder = appearance.ui_builder();

        render_body_item::<AndroidStudioPageAction>(
            "Android Studio Mode".into(),
            None,
            LocalOnlyIconState::default(),
            ToggleState::Enabled,
            appearance,
            ui_builder
                .switch(self.switch_state.clone())
                .check(true)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(AndroidStudioPageAction::Toggle);
                })
                .finish(),
            Some("Enable Android development features: build toolbar, logcat panel, screen mirroring".into()),
        )
    }
}
