use super::{
    settings_page::{
        MatchData, PageType, SettingsPageMeta, SettingsPageViewHandle,
    },
    SettingsSection,
};
use warp_core::features::FeatureFlag;
use warpui::{
    elements::Element,
    AppContext, Entity, TypedActionView, View, ViewContext,
};

#[derive(Debug, Clone)]
pub enum AndroidStudioPageAction {
    /// No actions yet — placeholder for future use.
    Placeholder,
}

pub enum AndroidStudioPageEvent {
    /// Placeholder event.
    Placeholder,
}

pub struct AndroidStudioPageView {
    page: PageType<Self>,
}

impl AndroidStudioPageView {
    pub fn new(_ctx: &mut ViewContext<Self>) -> Self {
        Self {
            page: PageType::new(
                vec![],
                super::settings_page::Category::new(
                    "Android Studio Features",
                    vec![],
                ),
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

    fn handle_action(&mut self, _action: &Self::Action, _ctx: &mut ViewContext<Self>) {}
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

    fn scroll_to_widget(&mut self, _widget_id: &'static str) {}
}
