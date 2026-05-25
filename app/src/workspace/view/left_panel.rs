use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use warp_core::send_telemetry_from_ctx;
use warp_core::ui::theme::color::internal_colors;
use warp_core::ui::Icon;
use warp_util::path::LineAndColumnArg;
use warpui::elements::{
    resizable_state_handle, ChildView, ConstrainedBox, Container, CrossAxisAlignment, DragBarSide,
    Element, Empty, Flex, MainAxisAlignment, MainAxisSize, MouseStateHandle, ParentElement,
    Resizable, ResizableStateHandle, Shrinkable,
};
use warpui::platform::Cursor;
use warpui::ui_components::components::{Coords, UiComponent, UiComponentStyles};
use warpui::{
    AppContext, Entity, FocusContext, ModelHandle, SingletonEntity, TypedActionView, View,
    ViewContext, ViewHandle, WeakViewHandle,
};
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent_conversations_model::AgentConversationsModel;
use crate::appearance::Appearance;
use crate::code::buffer_location::LocalOrRemotePath;
#[cfg(feature = "local_fs")]
use crate::code::file_tree::FileTreeEvent;
use crate::code::file_tree::FileTreeView;
use crate::coding_panel_enablement_state::CodingPanelEnablementState;
use crate::drive::panel::{
    DrivePanel, DrivePanelEvent, MAX_SIDEBAR_WIDTH_RATIO, MIN_SIDEBAR_WIDTH,
};
use crate::pane_group::pane::view::header::components::HEADER_EDGE_PADDING;
use crate::pane_group::pane::view::header::PANE_HEADER_HEIGHT;
use crate::pane_group::working_directories::WorkingDirectory;
use crate::pane_group::{
    PaneGroup, WorkingDirectoriesEvent, WorkingDirectoriesModel, {self},
};
#[cfg(feature = "local_fs")]
use crate::server::telemetry::CodePanelsFileOpenEntrypoint;
use crate::server::telemetry::{FileTreeSource, WarpDriveSource};
use crate::settings_view::keybindings::{KeybindingChangedEvent, KeybindingChangedNotifier};
use crate::terminal::resizable_data::{ModalType, ResizableData};
use crate::ui_components::buttons::{icon_button, icon_button_with_color};
use crate::ui_components::icons;
use crate::util::bindings::keybinding_name_to_display_string;
#[cfg(feature = "local_fs")]
use crate::util::file::external_editor::EditorSettings;
#[cfg(feature = "local_fs")]
use crate::util::openable_file_type::resolve_file_target_with_editor_choice;
use crate::util::openable_file_type::FileTarget;
use crate::workspace::view::conversation_list::view::{
    ConversationListView, Event as ConversationListViewEvent,
};
use crate::workspace::view::global_search::view::{
    Event as GlobalSearchViewEvent, GlobalSearchEntryFocus, GlobalSearchView,
};
use crate::workspace::view::{
    LEFT_PANEL_AGENT_CONVERSATIONS_BINDING_NAME, LEFT_PANEL_GLOBAL_SEARCH_BINDING_NAME,
    LEFT_PANEL_PROJECT_EXPLORER_BINDING_NAME, LEFT_PANEL_WARP_DRIVE_BINDING_NAME,
    OPEN_GLOBAL_SEARCH_BINDING_NAME, TOGGLE_CONVERSATION_LIST_VIEW_BINDING_NAME,
    TOGGLE_PROJECT_EXPLORER_BINDING_NAME, TOGGLE_WARP_DRIVE_BINDING_NAME,
};
use crate::workspace::WorkspaceAction;
use crate::TelemetryEvent;

#[derive(Default)]
struct MouseStateHandles {
    project_explorer_button: MouseStateHandle,
    global_search_button: MouseStateHandle,
    warp_drive_button: MouseStateHandle,
    conversation_list_view_button: MouseStateHandle,
    android_run_button: MouseStateHandle,
    android_device_label: MouseStateHandle,
}

#[derive(Clone, Debug)]
pub enum LeftPanelAction {
    ProjectExplorer,
    GlobalSearch { entry_focus: GlobalSearchEntryFocus },
    WarpDrive,
    ConversationListView,
    RunBuild {
        project_dir: std::path::PathBuf,
        serial: String,
    },
}

#[allow(clippy::large_enum_variant)]
pub enum LeftPanelEvent {
    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    FileTree(pane_group::Event),
    WarpDrive(DrivePanelEvent),
    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    OpenFileWithTarget {
        location: LocalOrRemotePath,
        target: FileTarget,
        line_col: Option<LineAndColumnArg>,
    },
    NewConversationInNewTab,
    ShowDeleteConfirmationDialog {
        conversation_id: AIConversationId,
        conversation_title: String,
        terminal_view_id: Option<warpui::EntityId>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolPanelView {
    ProjectExplorer,
    GlobalSearch { entry_focus: GlobalSearchEntryFocus },
    WarpDrive,
    ConversationListView,
}

/// Encapsulates the active view state to enforce that all mutations go through
/// `active_view_state::set`, which handles necessary side effects.
mod active_view_state {
    use warpui::ViewContext;

    use super::ToolPanelView;

    pub struct ActiveViewState(ToolPanelView);

    impl ActiveViewState {
        pub fn get(&self) -> ToolPanelView {
            self.0
        }
    }

    pub fn new(view: ToolPanelView) -> ActiveViewState {
        ActiveViewState(view)
    }

    pub fn set(
        left_panel: &mut super::LeftPanelView,
        new_view: ToolPanelView,
        ctx: &mut ViewContext<super::LeftPanelView>,
    ) {
        let previous = left_panel.active_view.0;
        left_panel.active_view.0 = new_view;
        left_panel.update_button_active_states();
        ctx.notify();

        let was_conversation_list_open = previous == ToolPanelView::ConversationListView;
        let is_conversation_list_open = new_view == ToolPanelView::ConversationListView;
        if was_conversation_list_open && !is_conversation_list_open {
            left_panel.on_conversation_list_view_visibility_changed(false, ctx);
        } else if !was_conversation_list_open && is_conversation_list_open {
            left_panel.on_conversation_list_view_visibility_changed(true, ctx);
        }

        left_panel.update_active_file_tree_subscription_state(ctx);
    }
}

pub struct ToolbeltButtonConfig {
    pub icon: warp_core::ui::Icon,
    /// Optional icon to use when the given toolbelt option is in an active state.
    pub active_icon: Option<warp_core::ui::Icon>,
    pub tooltip_text: String,
    pub action: LeftPanelAction,
    /// Whether the button should be rendered with an "active" state.
    pub render_with_active_state: bool,
    /// Ordered list of binding names used to populate the tooltip keybinding display.
    ///
    /// Earlier bindings in the list are preferred in the tooltip.
    pub tooltip_keybinding_names: Vec<&'static str>,
    /// Cached keybinding display string for the tooltip.
    ///
    /// This is updated in response to [`KeybindingChangedEvent`]s.
    pub tooltip_keybinding: Option<String>,
}

/// Device list shared between the UI thread and a background watcher thread.
#[derive(Default)]
struct SharedAndroidState {
    devices: Vec<crate::android::device::AndroidDevice>,
    selected_index: usize,
}

pub struct LeftPanelView {
    resizable_state_handle: ResizableStateHandle,
    mouse_state_handles: MouseStateHandles,
    close_button_mouse_state: MouseStateHandle,
    warp_drive_view: ViewHandle<DrivePanel>,
    conversation_list_view: ViewHandle<ConversationListView>,
    active_view: active_view_state::ActiveViewState,
    toolbelt_buttons: Vec<ToolbeltButtonConfig>,
    android_state: Arc<Mutex<SharedAndroidState>>,
    active_pane_group: Option<WeakViewHandle<PaneGroup>>,
    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    working_directories_model: ModelHandle<WorkingDirectoriesModel>,
    is_agent_management_view_open: bool,
    panel_position: super::PanelPosition,
}

fn toolbelt_tooltip_keybinding(binding_names: &[&'static str], app: &AppContext) -> Option<String> {
    let mut parts = Vec::new();
    let mut seen = HashSet::new();

    // Preserve caller-provided ordering so we can prioritize specific bindings.
    for binding_name in binding_names {
        if let Some(displayed) = keybinding_name_to_display_string(binding_name, app) {
            if seen.insert(displayed.clone()) {
                parts.push(displayed);
            }
        }
    }

    (!parts.is_empty()).then(|| parts.join(", "))
}

impl LeftPanelView {
    pub fn new(
        working_directories_model: ModelHandle<WorkingDirectoriesModel>,
        views: Vec<ToolPanelView>,
        ctx: &mut ViewContext<Self>,
    ) -> Self {
        let resizable_data_handle = ResizableData::handle(ctx);
        let resizable_state_handle = match resizable_data_handle
            .as_ref(ctx)
            .get_handle(ctx.window_id(), ModalType::LeftPanelWidth)
        {
            Some(handle) => handle,
            None => {
                log::error!("Couldn't retrieve left panel resizable state handle.");
                resizable_state_handle(600.0)
            }
        };
        let warp_drive_view = ctx.add_typed_action_view(DrivePanel::new);
        let conversation_list_view = ctx.add_typed_action_view(ConversationListView::new);

        ctx.subscribe_to_view(&warp_drive_view, |_me, _, event, ctx| {
            ctx.emit(LeftPanelEvent::WarpDrive(event.clone()));
        });

        ctx.subscribe_to_view(&conversation_list_view, |_me, _, event, ctx| match event {
            ConversationListViewEvent::NewConversationInNewTab => {
                ctx.emit(LeftPanelEvent::NewConversationInNewTab);
            }
            ConversationListViewEvent::ShowDeleteConfirmationDialog {
                conversation_id,
                conversation_title,
                terminal_view_id,
            } => {
                ctx.emit(LeftPanelEvent::ShowDeleteConfirmationDialog {
                    conversation_id: *conversation_id,
                    conversation_title: conversation_title.clone(),
                    terminal_view_id: *terminal_view_id,
                });
            }
        });

        // Android: background thread watches USB via inotify (event-driven, no polling)
        let android_state = Arc::new(Mutex::new(SharedAndroidState::default()));
        {
            let state = Arc::clone(&android_state);
            std::thread::spawn(move || {
                Self::usb_watcher_thread(state);
            });
        }

        let active_view = views.first().copied().unwrap_or(ToolPanelView::WarpDrive);
        let toolbelt_buttons = views
            .iter()
            .map(|view| Self::create_toolbelt_button_config(view, ctx))
            .collect();

        ctx.subscribe_to_model(
            &KeybindingChangedNotifier::handle(ctx),
            |me, _, event, ctx| match event {
                KeybindingChangedEvent::BindingChanged { .. } => {
                    for button in &mut me.toolbelt_buttons {
                        button.tooltip_keybinding =
                            toolbelt_tooltip_keybinding(&button.tooltip_keybinding_names, ctx);
                    }

                    ctx.notify();
                }
            },
        );

        ctx.subscribe_to_model(&working_directories_model, |me, _, event, ctx| {
            if let WorkingDirectoriesEvent::DirectoriesChanged {
                pane_group_id,
                directories,
            } = event
            {
                let Some(active_pane_group) = &me.active_pane_group else {
                    return;
                };
                let Some(active_pane_group) = active_pane_group.upgrade(ctx) else {
                    return;
                };
                if active_pane_group.id() != *pane_group_id {
                    return;
                }
                let has_terminal_session = directories.iter().any(|dir| dir.terminal_id.is_some());

                // Split directories into local and remote.
                let local_paths: Vec<PathBuf> = directories
                    .iter()
                    .filter_map(|d| d.path.to_local_path().map(|p| p.to_path_buf()))
                    .collect();
                #[allow(unused_variables)]
                let remote_repos: Vec<repo_metadata::RemoteRepositoryIdentifier> = directories
                    .iter()
                    .filter_map(|d| match &d.path {
                        LocalOrRemotePath::Remote(remote_path) => {
                            Some(repo_metadata::RemoteRepositoryIdentifier::new(
                                remote_path.host_id.clone(),
                                remote_path.path.clone(),
                            ))
                        }
                        _ => None,
                    })
                    .collect();

                // Update GlobalSearchView root directories (local only).
                let global_search_view =
                    me.get_or_create_global_search_view_for_pane_group(active_pane_group.id(), ctx);
                global_search_view.update(ctx, |view, view_ctx| {
                    view.set_root_directories(local_paths.clone(), view_ctx);
                });

                // Directories are already in display order (most recent first) from the model
                let local_directories = deduplicate_by_directory_name(local_paths);
                let file_tree_view =
                    me.get_or_create_file_tree_view_for_pane_group(active_pane_group.id(), ctx);

                let is_visible =
                    active_pane_group.as_ref(ctx).left_panel_open && me.is_file_tree_active();
                file_tree_view.update(ctx, |view, ctx| {
                    view.set_root_directories(local_directories, ctx);
                    #[cfg(feature = "local_fs")]
                    view.set_remote_root_directories(&remote_repos, ctx);
                    view.set_has_terminal_session(has_terminal_session, ctx);
                    view.set_is_active(is_visible, ctx);

                    if is_visible {
                        view.auto_expand_to_most_recent_directory(ctx);
                    }
                });
                ctx.notify();
            }
        });

        let mut view = Self {
            resizable_state_handle,
            mouse_state_handles: Default::default(),
            close_button_mouse_state: Default::default(),
            warp_drive_view,
            conversation_list_view,
            android_state,
            active_view: active_view_state::new(active_view),
            toolbelt_buttons,
            active_pane_group: None,
            working_directories_model,
            is_agent_management_view_open: false,
            panel_position: super::PanelPosition::Left,
        };
        view.update_button_active_states();

        view
    }

    pub fn set_agent_management_view_open(&mut self, is_open: bool, ctx: &mut ViewContext<Self>) {
        self.is_agent_management_view_open = is_open;
        ctx.notify();
    }

    pub fn set_panel_position(
        &mut self,
        position: super::PanelPosition,
        ctx: &mut ViewContext<Self>,
    ) {
        self.panel_position = position;
        ctx.notify();
    }

    /// Updates the available tool panel views.
    /// If the currently active view is no longer available, switches to the first available view.
    pub fn update_available_views(
        &mut self,
        views: Vec<ToolPanelView>,
        ctx: &mut ViewContext<Self>,
    ) {
        // Check if the current active view is still available
        let current_view = self.active_view.get();
        let is_current_view_available = views.iter().any(|v| {
            // Use discriminant comparison for GlobalSearch since it has inner data
            match (v, &current_view) {
                (ToolPanelView::GlobalSearch { .. }, ToolPanelView::GlobalSearch { .. }) => true,
                _ => std::mem::discriminant(v) == std::mem::discriminant(&current_view),
            }
        });

        // Rebuild toolbelt buttons
        self.toolbelt_buttons = views
            .iter()
            .map(|view| Self::create_toolbelt_button_config(view, ctx))
            .collect();

        // If current view is no longer available, switch to the first available view
        if !is_current_view_available {
            if let Some(first_view) = views.first().copied() {
                active_view_state::set(self, first_view, ctx);
            }
        } else {
            self.update_button_active_states();
        }

        ctx.notify();
    }

    fn create_toolbelt_button_config(
        view: &ToolPanelView,
        ctx: &ViewContext<Self>,
    ) -> ToolbeltButtonConfig {
        match view {
            ToolPanelView::ProjectExplorer => {
                let tooltip_keybinding_names = vec![
                    LEFT_PANEL_PROJECT_EXPLORER_BINDING_NAME,
                    TOGGLE_PROJECT_EXPLORER_BINDING_NAME,
                ];

                ToolbeltButtonConfig {
                    icon: Icon::FileCopy,
                    active_icon: None,
                    tooltip_text: "Project explorer".to_string(),
                    action: LeftPanelAction::ProjectExplorer,
                    render_with_active_state: false,
                    tooltip_keybinding: toolbelt_tooltip_keybinding(&tooltip_keybinding_names, ctx),
                    tooltip_keybinding_names,
                }
            }
            ToolPanelView::GlobalSearch { .. } => {
                let tooltip_keybinding_names = vec![
                    LEFT_PANEL_GLOBAL_SEARCH_BINDING_NAME,
                    OPEN_GLOBAL_SEARCH_BINDING_NAME,
                ];

                ToolbeltButtonConfig {
                    icon: Icon::Search,
                    active_icon: None,
                    tooltip_text: "Global search".to_string(),
                    action: LeftPanelAction::GlobalSearch {
                        entry_focus: GlobalSearchEntryFocus::QueryEditor,
                    },
                    render_with_active_state: false,
                    tooltip_keybinding: toolbelt_tooltip_keybinding(&tooltip_keybinding_names, ctx),
                    tooltip_keybinding_names,
                }
            }
            ToolPanelView::WarpDrive => {
                let tooltip_keybinding_names = vec![
                    LEFT_PANEL_WARP_DRIVE_BINDING_NAME,
                    TOGGLE_WARP_DRIVE_BINDING_NAME,
                ];

                ToolbeltButtonConfig {
                    icon: Icon::WarpDrive,
                    active_icon: None,
                    tooltip_text: "Warp Drive".to_string(),
                    action: LeftPanelAction::WarpDrive,
                    render_with_active_state: false,
                    tooltip_keybinding: toolbelt_tooltip_keybinding(&tooltip_keybinding_names, ctx),
                    tooltip_keybinding_names,
                }
            }
            ToolPanelView::ConversationListView => {
                let tooltip_keybinding_names = vec![
                    LEFT_PANEL_AGENT_CONVERSATIONS_BINDING_NAME,
                    TOGGLE_CONVERSATION_LIST_VIEW_BINDING_NAME,
                ];

                ToolbeltButtonConfig {
                    icon: Icon::Conversation,
                    active_icon: Some(Icon::Conversation),
                    tooltip_text: "Agent conversations".to_string(),
                    action: LeftPanelAction::ConversationListView,
                    render_with_active_state: false,
                    tooltip_keybinding: toolbelt_tooltip_keybinding(&tooltip_keybinding_names, ctx),
                    tooltip_keybinding_names,
                }
            }
        }
    }

    fn get_or_create_global_search_view_for_pane_group(
        &mut self,
        pane_group_id: warpui::EntityId,
        ctx: &mut ViewContext<Self>,
    ) -> ViewHandle<GlobalSearchView> {
        if let Some(view) = self
            .working_directories_model
            .as_ref(ctx)
            .get_global_search_view(pane_group_id)
        {
            return view;
        }

        let global_search_view = ctx.add_typed_action_view(GlobalSearchView::new);

        ctx.subscribe_to_view(&global_search_view, |me, _, event, ctx| {
            me.handle_global_search_event(event, ctx);
        });

        self.working_directories_model.update(ctx, |model, _ctx| {
            model.store_global_search_view(pane_group_id, global_search_view.clone());
        });

        global_search_view
    }

    fn get_or_create_file_tree_view_for_pane_group(
        &mut self,
        pane_group_id: warpui::EntityId,
        ctx: &mut ViewContext<Self>,
    ) -> ViewHandle<FileTreeView> {
        if let Some(view) = self
            .working_directories_model
            .as_ref(ctx)
            .get_file_tree_view(pane_group_id)
        {
            return view;
        }

        let file_tree_view = ctx.add_typed_action_view(FileTreeView::new);

        #[cfg(feature = "local_fs")]
        ctx.subscribe_to_view(&file_tree_view, |me, _, event, ctx| {
            me.handle_file_tree_event(event, ctx);
        });

        self.working_directories_model.update(ctx, |model, _ctx| {
            model.store_file_tree_view(pane_group_id, file_tree_view.clone());
        });

        file_tree_view
    }

    pub fn active_global_search_view(
        &self,
        app: &AppContext,
    ) -> Option<ViewHandle<GlobalSearchView>> {
        let pane_group_id = self
            .active_pane_group
            .as_ref()
            .and_then(|pane_group| pane_group.upgrade(app))
            .map(|pane_group| pane_group.id())?;
        self.working_directories_model
            .as_ref(app)
            .get_global_search_view(pane_group_id)
    }

    fn active_file_tree_view(&self, app: &AppContext) -> Option<ViewHandle<FileTreeView>> {
        let pane_group_id = self
            .active_pane_group
            .as_ref()
            .and_then(|pane_group| pane_group.upgrade(app))
            .map(|pane_group| pane_group.id())?;
        self.working_directories_model
            .as_ref(app)
            .get_file_tree_view(pane_group_id)
    }

    pub fn active_view(&self) -> ToolPanelView {
        self.active_view.get()
    }

    pub fn is_warp_drive_active(&self) -> bool {
        self.active_view.get() == ToolPanelView::WarpDrive
    }

    pub fn is_file_tree_active(&self) -> bool {
        self.active_view.get() == ToolPanelView::ProjectExplorer
    }

    pub fn warp_drive_view(&self) -> &ViewHandle<DrivePanel> {
        &self.warp_drive_view
    }

    pub(crate) fn auto_expand_active_file_tree_to_most_recent_directory(
        &mut self,
        ctx: &mut ViewContext<Self>,
    ) {
        if let Some(file_tree_view) = self.active_file_tree_view(ctx) {
            file_tree_view.update(ctx, |view, ctx| {
                view.auto_expand_to_most_recent_directory(ctx);
            });
        }
    }

    pub fn restore_active_view_from_snapshot(
        &mut self,
        view: ToolPanelView,
        ctx: &mut ViewContext<Self>,
    ) {
        active_view_state::set(self, view, ctx);
    }

    /// Updates the active pane group ID so we filter events correctly.
    pub fn set_active_pane_group(
        &mut self,
        pane_group: ViewHandle<PaneGroup>,
        working_directories_model: &ModelHandle<WorkingDirectoriesModel>,
        ctx: &mut ViewContext<Self>,
    ) {
        let pane_group_id = pane_group.id();

        let previous_pane_group_id = self
            .active_pane_group
            .as_ref()
            .and_then(|pane_group| pane_group.upgrade(ctx))
            .map(|pane_group| pane_group.id());

        self.active_pane_group = Some(pane_group.downgrade());

        if let Some(previous_pane_group_id) = previous_pane_group_id {
            if previous_pane_group_id != pane_group_id {
                self.deactivate_file_tree_view_for_pane_group(previous_pane_group_id, ctx);
            }
        }

        // Query the current state from the model
        let active_directories: Vec<WorkingDirectory> =
            working_directories_model.read(ctx, |model, _| {
                model
                    .most_recent_directories_for_pane_group(pane_group_id)
                    .map(|dirs| dirs.collect())
                    .unwrap_or_default()
            });
        let has_terminal_session = active_directories
            .iter()
            .any(|dir| dir.terminal_id.is_some());

        // Split directories into local and remote.
        let local_paths: Vec<PathBuf> = active_directories
            .iter()
            .filter_map(|d| d.path.to_local_path().map(|p| p.to_path_buf()))
            .collect();
        #[allow(unused_variables)]
        let remote_repos: Vec<repo_metadata::RemoteRepositoryIdentifier> = active_directories
            .iter()
            .filter_map(|d| match &d.path {
                LocalOrRemotePath::Remote(remote_path) => {
                    Some(repo_metadata::RemoteRepositoryIdentifier::new(
                        remote_path.host_id.clone(),
                        remote_path.path.clone(),
                    ))
                }
                _ => None,
            })
            .collect();

        // Update GlobalSearchView root directories (local only).
        let global_search_view =
            self.get_or_create_global_search_view_for_pane_group(pane_group_id, ctx);
        global_search_view.update(ctx, |view, view_ctx| {
            view.set_root_directories(local_paths.clone(), view_ctx);
        });

        let local_directories = deduplicate_by_directory_name(local_paths);
        let active_file_model = pane_group.as_ref(ctx).active_file_model().clone();

        let file_tree_view = self.get_or_create_file_tree_view_for_pane_group(pane_group_id, ctx);
        let left_panel_open = pane_group.as_ref(ctx).left_panel_open;
        let is_visible = left_panel_open && self.is_file_tree_active();
        file_tree_view.update(ctx, |view, ctx| {
            view.set_root_directories(local_directories, ctx);
            #[cfg(feature = "local_fs")]
            view.set_remote_root_directories(&remote_repos, ctx);
            view.set_has_terminal_session(has_terminal_session, ctx);
            view.set_active_file_model(active_file_model, ctx);
            view.set_is_active(is_visible, ctx);

            if is_visible {
                view.auto_expand_to_most_recent_directory(ctx);
            }
        });

        self.on_left_panel_visibility_changed(left_panel_open, ctx);

        ctx.notify();
    }

    pub fn update_coding_panel_enablement(
        &mut self,
        enablement: CodingPanelEnablementState,
        ctx: &mut ViewContext<Self>,
    ) {
        #[cfg(feature = "local_fs")]
        {
            if let Some(file_tree_view) = self.active_file_tree_view(ctx) {
                file_tree_view.update(ctx, |view, ctx| {
                    view.set_enablement_state(enablement, ctx);
                });
            }
        }

        if let Some(global_search_view) = self.active_global_search_view(ctx) {
            global_search_view.update(ctx, |view, view_ctx| {
                view.set_enablement_state(enablement, view_ctx);
            });
        }
    }

    pub fn focus_active_view_on_entry(&mut self, ctx: &mut ViewContext<Self>) {
        match self.active_view.get() {
            ToolPanelView::ProjectExplorer => {
                if let Some(file_tree_view) = self.active_file_tree_view(ctx) {
                    file_tree_view.update(ctx, |view, ctx| {
                        view.on_left_panel_focused(ctx);
                    });
                    ctx.focus(&file_tree_view);
                }
            }
            ToolPanelView::GlobalSearch { entry_focus } => {
                if let Some(global_search_view) = self.active_global_search_view(ctx) {
                    global_search_view.update(ctx, |view, ctx| {
                        view.on_left_panel_focused(entry_focus, ctx);
                    });
                }

                active_view_state::set(
                    self,
                    ToolPanelView::GlobalSearch {
                        entry_focus: GlobalSearchEntryFocus::Results,
                    },
                    ctx,
                );
            }
            ToolPanelView::WarpDrive => {
                ctx.focus(&self.warp_drive_view);
                self.warp_drive_view.update(ctx, |view, ctx| {
                    view.reset_focused_index_in_warp_drive(true, ctx);
                });
            }
            ToolPanelView::ConversationListView => {
                self.conversation_list_view.update(ctx, |view, ctx| {
                    view.on_left_panel_focused(ctx);
                });
            }
        }
    }

    #[cfg(not(feature = "local_fs"))]
    fn handle_global_search_event(
        &mut self,
        _event: &GlobalSearchViewEvent,
        _ctx: &mut ViewContext<Self>,
    ) {
    }

    #[cfg(feature = "local_fs")]
    fn handle_global_search_event(
        &mut self,
        event: &GlobalSearchViewEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        match event {
            GlobalSearchViewEvent::OpenMatch {
                path,
                line_number,
                column_num,
            } => {
                let line_col = LineAndColumnArg {
                    line_num: *line_number as usize,
                    column_num: *column_num,
                };

                let settings = EditorSettings::as_ref(ctx);
                let target = resolve_file_target_with_editor_choice(
                    path,
                    *settings.open_code_panels_file_editor,
                    *settings.prefer_markdown_viewer,
                    *settings.open_file_layout,
                    None,
                );

                send_telemetry_from_ctx!(
                    TelemetryEvent::CodePanelsFileOpened {
                        entrypoint: CodePanelsFileOpenEntrypoint::GlobalSearch,
                        target: target.clone(),
                    },
                    ctx
                );

                ctx.emit(LeftPanelEvent::OpenFileWithTarget {
                    location: LocalOrRemotePath::Local(path.clone()),
                    target,
                    line_col: Some(line_col),
                });
            }
        }
    }

    #[cfg(feature = "local_fs")]
    fn handle_file_tree_event(&mut self, event: &FileTreeEvent, ctx: &mut ViewContext<Self>) {
        match event {
            FileTreeEvent::FileRenamed { old_path, new_path } => {
                ctx.emit(LeftPanelEvent::FileTree(pane_group::Event::FileRenamed {
                    old_path: old_path.clone(),
                    new_path: new_path.clone(),
                }));
            }
            FileTreeEvent::FileDeleted { path } => {
                ctx.emit(LeftPanelEvent::FileTree(pane_group::Event::FileDeleted {
                    path: path.clone(),
                }));
            }
            FileTreeEvent::AttachAsContext { path } => {
                ctx.emit(LeftPanelEvent::FileTree(
                    pane_group::Event::AttachPathAsContext { path: path.clone() },
                ));
            }
            FileTreeEvent::OpenFile {
                path,
                target,
                line_col,
            } => {
                ctx.emit(LeftPanelEvent::OpenFileWithTarget {
                    location: path.clone(),
                    target: target.clone(),
                    line_col: *line_col,
                });
            }
            FileTreeEvent::CDToDirectory { path } => {
                ctx.emit(LeftPanelEvent::FileTree(pane_group::Event::CDToDirectory {
                    path: path.clone(),
                }));
            }
            FileTreeEvent::OpenDirectoryInNewTab { path } => {
                ctx.emit(LeftPanelEvent::FileTree(
                    pane_group::Event::OpenDirectoryInNewTab { path: path.clone() },
                ));
            }
        }
    }
}

impl Entity for LeftPanelView {
    type Event = LeftPanelEvent;
}

impl LeftPanelView {
    fn close_button(&self, appearance: &Appearance, app: &AppContext) -> Box<dyn Element> {
        let ui_builder = appearance.ui_builder().clone();
        let tooltip_keybinding =
            keybinding_name_to_display_string("workspace:toggle_left_panel", app);

        let tooltip = if let Some(keybinding) = tooltip_keybinding {
            ui_builder
                .tool_tip_with_sublabel("Close panel".to_string(), keybinding)
                .build()
                .finish()
        } else {
            ui_builder
                .tool_tip("Close panel".to_string())
                .build()
                .finish()
        };

        let icon_color = appearance
            .theme()
            .sub_text_color(appearance.theme().background());
        icon_button_with_color(
            appearance,
            icons::Icon::X,
            false,
            self.close_button_mouse_state.clone(),
            icon_color,
        )
        .with_tooltip(move || tooltip)
        .build()
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(WorkspaceAction::ToggleLeftPanel);
        })
        .with_cursor(Cursor::PointingHand)
        .finish()
    }

    /// Background thread: watch USB hotplug via inotify (Linux) or polling (other platforms).
    fn usb_watcher_thread(state: Arc<Mutex<SharedAndroidState>>) {
        Self::update_device_list(&state);

        #[cfg(target_os = "linux")]
        {
            match Self::try_inotify_watch(&state) {
                Ok(()) => return,
                Err(e) => {
                    log::warn!("Android: inotify unavailable ({e}), falling back to polling");
                }
            }
        }

        // Polling fallback (always used on Windows/macOS).
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            Self::update_device_list(&state);
        }
    }

    #[cfg(target_os = "linux")]
    fn try_inotify_watch(state: &Arc<Mutex<SharedAndroidState>>) -> Result<(), String> {
        use std::os::fd::AsRawFd;

        let mut inotify =
            inotify::Inotify::init().map_err(|e| format!("inotify init: {e}"))?;

        // USB device nodes live in /dev/bus/usb/XXX/ subdirectories.
        // Watch each subdirectory for CREATE/DELETE of device files.
        let watch_mask =
            inotify::WatchMask::CREATE | inotify::WatchMask::DELETE;
        let mut watch_count = 0;
        if let Ok(entries) = std::fs::read_dir("/dev/bus/usb") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    inotify
                        .watches()
                        .add(&path, watch_mask)
                        .map_err(|e| format!("watch {}: {e}", path.display()))?;
                    watch_count += 1;
                }
            }
        }
        if watch_count == 0 {
            return Err("no /dev/bus/usb subdirectories found".into());
        }

        let fd = inotify.as_raw_fd();
        let mut buffer = [0u8; 4096];
        let mut pollfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };

        loop {
            if unsafe { libc::poll(&mut pollfd, 1, -1) } < 0 {
                return Err("poll error".into());
            }
            let events = inotify
                .read_events(&mut buffer)
                .map_err(|e| format!("read_events: {e}"))?;
            let count = events.count();
            if count > 0 {
                std::thread::sleep(std::time::Duration::from_millis(600));
                Self::update_device_list(state);
            }
        }
    }

    /// Run `adb devices -l` and update shared state if device list changed.
    fn update_device_list(state: &Arc<Mutex<SharedAndroidState>>) {
        use crate::android::device::AdbDeviceService;
        let devices: Vec<_> = AdbDeviceService::list_devices_cli()
            .unwrap_or_default()
            .into_iter()
            .filter(|d| d.state == "device")
            .collect();
        if let Ok(mut s) = state.lock() {
            if s.devices.len() != devices.len()
                || s.devices.iter().zip(&devices).any(|(a, b)| a.serial != b.serial)
            {
                s.devices = devices.clone();
                if s.selected_index >= s.devices.len().max(1) {
                    s.selected_index = 0;
                }
            }
        }
        // Also sync to global LOGCAT_STATE so LogcatView can access the device list.
        if let Ok(mut ls) = crate::android::logcat_state::LOGCAT_STATE.lock() {
            ls.devices = devices.clone();
        }
    }

    /// Runs the full build+install+launch pipeline and writes output to BUILD_STATE.
    fn run_build_with_output(project_dir: std::path::PathBuf, device_serial: String) {
        use crate::android::build_state::BUILD_STATE;
        use crate::android::runner::AndroidRunService;

        log::info!("[Android] run_build_with_output: dir={project_dir:?}, serial={device_serial}");

        {
            let mut state = BUILD_STATE.lock().unwrap();
            state.lines.clear();
            state.running = true;
            state.success = None;
            state.lines.push("Starting build...".into());
        }

        let runner = AndroidRunService::new(project_dir);
        let gradle = runner.gradle();

        // Step 1: assembleDebug (streaming)
        {
            let mut state = BUILD_STATE.lock().unwrap();
            state.lines.push(if cfg!(windows) {
                "> gradlew.bat assembleDebug".into()
            } else {
                "$ ./gradlew assembleDebug".into()
            });
        }
        eprintln!("[Android] Starting assembleDebug (streaming)...");
        match gradle.assemble_debug_streaming(|line| {
            if let Ok(mut state) = BUILD_STATE.lock() {
                state.lines.push(line);
            }
        }) {
            Ok(r) => {
                eprintln!("[Android] assembleDebug finished: success={}", r.success);
                let mut state = BUILD_STATE.lock().unwrap();
                if r.success {
                    state.lines.push("assembleDebug: SUCCESS".into());
                } else {
                    state.lines.push(format!(
                        "assembleDebug: FAILED (exit code {:?})",
                        r.exit_code
                    ));
                    state.running = false;
                    state.success = Some(false);
                    eprintln!("[Android] assembleDebug FAILED, lines={}", state.lines.len());
                    return;
                }
            }
            Err(e) => {
                let mut state = BUILD_STATE.lock().unwrap();
                state.lines.push(format!("Gradle error: {e}"));
                state.running = false;
                state.success = Some(false);
                eprintln!("[Android] Gradle error: {e}");
                return;
            }
        }

        // Step 2: Find APK
        {
            let mut state = BUILD_STATE.lock().unwrap();
            state.lines.push("Looking for APK...".into());
        }
        let apk_path = match runner.find_apk() {
            Ok(p) => {
                let mut state = BUILD_STATE.lock().unwrap();
                state.lines.push(format!("APK found: {}", p.display()));
                p
            }
            Err(e) => {
                let mut state = BUILD_STATE.lock().unwrap();
                state.lines.push(format!("ERROR: {e}"));
                state.running = false;
                state.success = Some(false);
                return;
            }
        };

        // Step 3: Extract app identity
        let identity = match runner.extract_app_identity(&apk_path) {
            Ok(id) => {
                let mut state = BUILD_STATE.lock().unwrap();
                state.lines.push(format!("Package: {}", id.package_name));
                if let Some(ref act) = id.launch_activity {
                    state.lines.push(format!("Activity: {act}"));
                }
                // Store package name in LOGCAT_STATE for PID-based filtering.
                if let Ok(mut ls) = crate::android::logcat_state::LOGCAT_STATE.lock() {
                    ls.package_name = Some(id.package_name.clone());
                }
                id
            }
            Err(e) => {
                let mut state = BUILD_STATE.lock().unwrap();
                state.lines.push(format!("ERROR: {e}"));
                state.running = false;
                state.success = Some(false);
                return;
            }
        };

        // Step 4: Install APK
        {
            let mut state = BUILD_STATE.lock().unwrap();
            state.lines.push(format!("Installing on {}...", device_serial));
        }
        if let Err(e) = runner.install_apk(&device_serial, &apk_path) {
            let mut state = BUILD_STATE.lock().unwrap();
            state.lines.push(format!("ADB install failed: {e}"));
            state.running = false;
            state.success = Some(false);
            return;
        }
        {
            let mut state = BUILD_STATE.lock().unwrap();
            state.lines.push("Install: SUCCESS".into());
        }

        // Step 5: Launch app
        {
            let mut state = BUILD_STATE.lock().unwrap();
            state.lines.push("Launching app...".into());
        }
        if let Err(e) = runner.launch_app(&device_serial, &identity) {
            let mut state = BUILD_STATE.lock().unwrap();
            state.lines.push(format!("Launch failed: {e}"));
            state.running = false;
            state.success = Some(false);
            return;
        }

        let mut state = BUILD_STATE.lock().unwrap();
        state.lines.push("App launched successfully!".into());
        state.running = false;
        state.success = Some(true);
        eprintln!("[Android] Build pipeline completed successfully, lines={}", state.lines.len());
    }

    fn render_android_toolbar(&self, appearance: &Appearance, app: &AppContext) -> Box<dyn Element> {
        use crate::android::gradle::GradleService;
        use crate::features::FeatureFlag;

        if !FeatureFlag::AndroidStudioMode.is_enabled() {
            return Empty::new().finish();
        }

        let theme = appearance.theme();

        // Read shared device state (lock is uncontended — only a 2s background thread)
        let (has_device, run_serial, device_text) = {
            let state = self.android_state.lock().unwrap();
            let devices = &state.devices;
            let selected_index = state.selected_index;
            let has_device = !devices.is_empty();
            let run_serial = if has_device {
                devices[selected_index.min(devices.len() - 1)].serial.clone()
            } else {
                String::new()
            };
            let device_text = if has_device {
                let d = &devices[selected_index.min(devices.len() - 1)];
                let label = match (&d.model, &d.product) {
                    (Some(m), _) if !m.is_empty() => m.clone(),
                    (_, Some(p)) if !p.is_empty() => p.clone(),
                    _ => d.serial.clone(),
                };
                if devices.len() > 1 {
                    format!("{} ({} of {})", label, selected_index + 1, devices.len())
                } else {
                    label
                }
            } else {
                "No device".to_string()
            };
            (has_device, run_serial, device_text)
        };

        let device_color = if has_device {
            theme.main_text_color(theme.background())
        } else {
            theme.disabled_text_color(theme.background())
        };

        // Clickable device label — cycles through devices
        let device_label_btn = appearance
            .ui_builder()
            .button(
                warpui::ui_components::button::ButtonVariant::Text,
                self.mouse_state_handles.android_device_label.clone(),
            )
            .with_style(warpui::ui_components::components::UiComponentStyles {
                font_size: Some(11.),
                padding: Some(Coords::default().left(4.).right(4.)),
                font_color: Some(device_color.into()),
                ..Default::default()
            })
            .with_text_label(device_text)
            .build()
            .on_click({
                let state = Arc::clone(&self.android_state);
                move |ctx, _, _| {
                    if let Ok(mut s) = state.lock() {
                        if s.devices.len() > 1 {
                            s.selected_index = (s.selected_index + 1) % s.devices.len();
                        }
                    }
                    ctx.notify();
                }
            })
            .with_cursor(Cursor::PointingHand)
            .finish();

        // Project detection: use the current terminal's working directory
        let cwd_location = self
            .active_pane_group
            .as_ref()
            .and_then(|pg| pg.upgrade(app))
            .and_then(|pg| {
                self.working_directories_model
                    .as_ref(app)
                    .most_recent_directories_for_pane_group(pg.id())
                    .and_then(|mut dirs| dirs.next().map(|d| d.path))
            });
        let cwd = cwd_location
            .as_ref()
            .and_then(|p| p.to_local_path().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let is_android = GradleService::new(cwd.clone()).has_gradlew();

        // Try to read package name from AndroidManifest.xml if not yet known.
        if is_android {
            if let Ok(mut ls) = crate::android::logcat_state::LOGCAT_STATE.lock() {
                if ls.package_name.is_none() {
                    ls.package_name =
                        crate::android::runner::read_package_name_from_manifest(&cwd);
                }
            }
        }

        let cwd_for_closure = cwd;
        let run_serial_for_closure = run_serial.clone();
        let can_run = has_device && is_android && !run_serial.is_empty();
        eprintln!("[Android] render_android_toolbar: has_device={has_device}, is_android={is_android}, run_serial={run_serial}, can_run={can_run}");

        // Run button
        let play_btn = appearance
            .ui_builder()
            .button(
                if can_run {
                    warpui::ui_components::button::ButtonVariant::Accent
                } else {
                    warpui::ui_components::button::ButtonVariant::Secondary
                },
                self.mouse_state_handles.android_run_button.clone(),
            )
            .with_text_label("Run".to_owned())
            .with_style(warpui::ui_components::components::UiComponentStyles {
                font_size: Some(11.),
                padding: Some(warpui::ui_components::components::Coords::default()
                    .left(6.).right(6.).top(2.).bottom(2.)),
                ..Default::default()
            })
            .build()
            .on_click(move |ctx, _, _| {
                eprintln!("[Android] Run button clicked, can_run={can_run}, has_device={has_device}, is_android={is_android}");
                if !can_run {
                    if !has_device {
                        log::warn!("Android: no device connected, cannot run");
                    } else if !is_android {
                        log::warn!("Android: not an Android project, cannot run");
                    }
                    return;
                }
                let serial = run_serial_for_closure.clone();
                let project_dir = cwd_for_closure.clone();
                log::info!("[Android] Dispatching RunBuild action, project_dir={project_dir:?}, serial={serial}");
                ctx.dispatch_typed_action(LeftPanelAction::RunBuild {
                    project_dir,
                    serial,
                });
            })
            .finish();

        let mut row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(6.);
        row.add_child(play_btn);
        row.add_child(device_label_btn);

        Container::new(row.finish())
            .with_padding_left(10.)
            .with_padding_bottom(6.)
            .with_padding_top(4.)
            .finish()
    }

    fn update_button_active_states(&mut self) {
        for button in &mut self.toolbelt_buttons {
            button.render_with_active_state = match &button.action {
                LeftPanelAction::ProjectExplorer => {
                    self.active_view.get() == ToolPanelView::ProjectExplorer
                }
                LeftPanelAction::GlobalSearch { .. } => {
                    matches!(self.active_view.get(), ToolPanelView::GlobalSearch { .. })
                }
                LeftPanelAction::WarpDrive => self.active_view.get() == ToolPanelView::WarpDrive,
                LeftPanelAction::ConversationListView => {
                    self.active_view.get() == ToolPanelView::ConversationListView
                }
                LeftPanelAction::RunBuild { .. } => false,
            };
        }
    }

    fn render_button(
        button_config: &ToolbeltButtonConfig,
        mouse_state: MouseStateHandle,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let action = button_config.action.clone();
        let ui_builder = appearance.ui_builder().clone();
        let tooltip_keybinding = button_config.tooltip_keybinding.clone();

        let icon_color = if button_config.render_with_active_state {
            appearance.theme().foreground().into_solid()
        } else {
            appearance
                .theme()
                .sub_text_color(appearance.theme().background())
                .into_solid()
        };

        let tooltip = if let Some(keybinding) = tooltip_keybinding {
            ui_builder
                .tool_tip_with_sublabel(button_config.tooltip_text.clone(), keybinding)
                .build()
                .finish()
        } else {
            ui_builder
                .tool_tip(button_config.tooltip_text.clone())
                .build()
                .finish()
        };

        let icon = if button_config.render_with_active_state {
            button_config.active_icon.unwrap_or(button_config.icon)
        } else {
            button_config.icon
        };

        icon_button(
            appearance,
            icon,
            button_config.render_with_active_state,
            mouse_state.clone(),
        )
        .with_tooltip(move || tooltip)
        .with_style(UiComponentStyles {
            font_color: Some(icon_color),
            height: Some(24.),
            width: Some(24.),
            padding: Some(Coords::uniform(4.)),
            ..Default::default()
        })
        .with_active_styles(UiComponentStyles {
            font_color: Some(icon_color),
            height: Some(24.),
            width: Some(24.),
            padding: Some(Coords::uniform(4.)),
            background: Some(internal_colors::fg_overlay_3(appearance.theme()).into()),
            ..Default::default()
        })
        .build()
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(action.clone());
        })
        .with_cursor(Cursor::PointingHand)
        .finish()
    }
}

impl LeftPanelView {
    pub fn handle_action_with_force_open(
        &mut self,
        action: &LeftPanelAction,
        force_open: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        match action {
            LeftPanelAction::ProjectExplorer => {
                active_view_state::set(self, ToolPanelView::ProjectExplorer, ctx);
                if force_open {
                    send_telemetry_from_ctx!(
                        TelemetryEvent::FileTreeToggled {
                            source: FileTreeSource::ForceOpened,
                            is_code_mode_v2: true,
                            cli_agent: None,
                        },
                        ctx
                    );
                } else {
                    send_telemetry_from_ctx!(
                        TelemetryEvent::FileTreeToggled {
                            source: FileTreeSource::LeftPanelToolbelt,
                            is_code_mode_v2: true,
                            cli_agent: None,
                        },
                        ctx
                    );
                }
            }
            LeftPanelAction::GlobalSearch { entry_focus } => {
                let was_active = self.active_view.get()
                    == ToolPanelView::GlobalSearch {
                        entry_focus: *entry_focus,
                    };
                active_view_state::set(
                    self,
                    ToolPanelView::GlobalSearch {
                        entry_focus: *entry_focus,
                    },
                    ctx,
                );
                if !was_active {
                    send_telemetry_from_ctx!(TelemetryEvent::GlobalSearchOpened, ctx);
                }
            }
            LeftPanelAction::WarpDrive => {
                active_view_state::set(self, ToolPanelView::WarpDrive, ctx);
                if force_open {
                    send_telemetry_from_ctx!(
                        TelemetryEvent::WarpDriveOpened {
                            source: WarpDriveSource::ForceOpened,
                            is_code_mode_v2: true
                        },
                        ctx
                    );
                } else {
                    send_telemetry_from_ctx!(
                        TelemetryEvent::WarpDriveOpened {
                            source: WarpDriveSource::LeftPanelToolbelt,
                            is_code_mode_v2: true
                        },
                        ctx
                    );
                }
            }
            LeftPanelAction::ConversationListView => {
                active_view_state::set(self, ToolPanelView::ConversationListView, ctx);
                send_telemetry_from_ctx!(TelemetryEvent::ConversationListViewOpened, ctx);
            }
            LeftPanelAction::RunBuild {
                project_dir,
                serial,
            } => {
                // Signal TerminalView to auto-open the build panel.
                if let Ok(mut bs) = crate::android::build_state::BUILD_STATE.lock() {
                    bs.should_open_panel = true;
                }
                let project_dir = project_dir.clone();
                let serial = serial.clone();
                ctx.spawn(
                    async move {
                        eprintln!("[Android] run_build_with_output started");
                        Self::run_build_with_output(project_dir, serial);
                        eprintln!("[Android] run_build_with_output finished");
                    },
                    |me, _, ctx| {
                        eprintln!("[Android] build spawn callback on main thread");
                        ctx.notify();
                        let _ = me;
                    },
                );
            }
        }
    }

    pub fn on_left_panel_visibility_changed(&self, is_now_open: bool, ctx: &mut ViewContext<Self>) {
        if ToolPanelView::ConversationListView == self.active_view.get() {
            self.on_conversation_list_view_visibility_changed(is_now_open, ctx);
        }

        self.update_active_file_tree_subscription_state(ctx);
    }

    fn deactivate_file_tree_view_for_pane_group(
        &self,
        pane_group_id: warpui::EntityId,
        ctx: &mut ViewContext<Self>,
    ) {
        if let Some(view) = self
            .working_directories_model
            .as_ref(ctx)
            .get_file_tree_view(pane_group_id)
        {
            view.update(ctx, |view, ctx| {
                view.set_is_active(false, ctx);
            });
        }
    }

    fn update_active_file_tree_subscription_state(&self, ctx: &mut ViewContext<Self>) {
        let Some(active_pane_group) = self
            .active_pane_group
            .as_ref()
            .and_then(|pane_group| pane_group.upgrade(ctx))
        else {
            return;
        };

        let is_visible = active_pane_group.as_ref(ctx).left_panel_open
            && self.active_view.get() == ToolPanelView::ProjectExplorer;

        if let Some(file_tree_view) = self
            .working_directories_model
            .as_ref(ctx)
            .get_file_tree_view(active_pane_group.id())
        {
            file_tree_view.update(ctx, |view, ctx| {
                view.set_is_active(is_visible, ctx);
            });
        }
    }

    /// When the conversation list view's visibility changes,
    /// we need to update the conversation and tasks model to reflect the new state
    /// (this information is used to decide whether or not we should poll for new tasks).
    fn on_conversation_list_view_visibility_changed(
        &self,
        is_now_open: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        let window_id = ctx.window_id();
        let view_id = self.conversation_list_view.id();
        AgentConversationsModel::handle(ctx).update(ctx, |model, ctx| {
            if is_now_open {
                model.register_view_open(window_id, view_id, ctx);
            } else {
                model.register_view_closed(window_id, view_id, ctx);
            }
        });
    }
}

impl TypedActionView for LeftPanelView {
    type Action = LeftPanelAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        self.handle_action_with_force_open(action, false, ctx);
    }
}

impl View for LeftPanelView {
    fn ui_name() -> &'static str {
        "LeftPanelView"
    }

    fn on_focus(&mut self, focus_ctx: &FocusContext, ctx: &mut ViewContext<Self>) {
        // Focus the active tool panel view on-left-panel-focus.
        if focus_ctx.is_self_focused() {
            match self.active_view.get() {
                ToolPanelView::ProjectExplorer => {
                    if let Some(view) = self.active_file_tree_view(ctx) {
                        ctx.focus(&view);
                    }
                }
                ToolPanelView::GlobalSearch { .. } => {
                    if let Some(view) = self.active_global_search_view(ctx) {
                        ctx.focus(&view);
                    }
                }
                ToolPanelView::WarpDrive => ctx.focus(&self.warp_drive_view),
                ToolPanelView::ConversationListView => ctx.focus(&self.conversation_list_view),
            }
        }
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);

        let mouse_state_handles = vec![
            self.mouse_state_handles.project_explorer_button.clone(),
            self.mouse_state_handles.global_search_button.clone(),
            self.mouse_state_handles.warp_drive_button.clone(),
            self.mouse_state_handles
                .conversation_list_view_button
                .clone(),
        ];

        // If there is only one button in the toolbelt row,
        // there is no need to show it as it's a bit redundant.
        let toolbelt_button_row = if self.toolbelt_buttons.len() > 1 {
            Some(
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_spacing(4.0)
                    .with_children(self.toolbelt_buttons.iter().zip(&mouse_state_handles).map(
                        |(button_config, mouse_state)| {
                            Self::render_button(button_config, mouse_state.clone(), appearance)
                        },
                    ))
                    .with_main_axis_size(MainAxisSize::Min)
                    .finish(),
            )
        } else {
            None
        };

        let content_area: Box<dyn Element> = match self.active_view.get() {
            ToolPanelView::ProjectExplorer => {
                if let Some(file_tree_view) = self.active_file_tree_view(app) {
                    Shrinkable::new(
                        1.0,
                        Container::new(ChildView::new(&file_tree_view).finish())
                            .with_padding_left(2.)
                            .with_padding_right(2.)
                            .finish(),
                    )
                    .finish()
                } else {
                    Shrinkable::new(1.0, Container::new(Empty::new().finish()).finish()).finish()
                }
            }
            ToolPanelView::GlobalSearch { .. } => {
                if let Some(global_search_view) = self.active_global_search_view(app) {
                    Shrinkable::new(
                        1.0,
                        Container::new(ChildView::new(&global_search_view).finish()).finish(),
                    )
                    .finish()
                } else {
                    Shrinkable::new(1.0, Container::new(Empty::new().finish()).finish()).finish()
                }
            }
            ToolPanelView::WarpDrive => Shrinkable::new(
                1.0,
                Container::new(ChildView::new(&self.warp_drive_view).finish())
                    .with_padding_left(2.)
                    .with_padding_right(2.)
                    .finish(),
            )
            .finish(),
            ToolPanelView::ConversationListView => {
                Shrinkable::new(1.0, ChildView::new(&self.conversation_list_view).finish()).finish()
            }
        };

        let panel_content = Container::new({
            let column = Flex::column();

            let header_left = if let Some(row) = toolbelt_button_row {
                row
            } else {
                Flex::row().finish()
            };

            let header_row = Container::new(
                ConstrainedBox::new(
                    Flex::row()
                        .with_main_axis_size(MainAxisSize::Max)
                        .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_child(Shrinkable::new(1.0, header_left).finish())
                        .with_child(self.close_button(appearance, app))
                        .finish(),
                )
                .with_height(PANE_HEADER_HEIGHT)
                .finish(),
            )
            .with_padding_left(10.)
            .with_padding_right(HEADER_EDGE_PADDING)
            .finish();

            column
                .with_child(header_row)
                .with_child(self.render_android_toolbar(appearance, app))
                .with_child(Shrinkable::new(1.0, content_area).finish())
                .with_main_axis_size(MainAxisSize::Max)
                .finish()
        })
        .finish();

        if warpui::platform::is_mobile_device() {
            return panel_content;
        }

        let drag_side = match self.panel_position {
            super::PanelPosition::Left => DragBarSide::Right,
            super::PanelPosition::Right => DragBarSide::Left,
        };
        Resizable::new(self.resizable_state_handle.clone(), panel_content)
            .with_dragbar_side(drag_side)
            .on_resize(move |ctx, _| {
                ctx.notify();
            })
            .with_bounds_callback(Box::new(|window_size| {
                let min_width = MIN_SIDEBAR_WIDTH;
                let max_width = window_size.x() * MAX_SIDEBAR_WIDTH_RATIO;
                (min_width, max_width.max(min_width))
            }))
            .finish()
    }
}

fn deduplicate_by_directory_name(directories: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    directories
        .into_iter()
        .filter(|path| seen_paths.insert(path.clone()))
        .collect()
}
