use std::sync::{Arc, LazyLock, Mutex};

/// Build output state shared between the background build thread and the UI.
#[derive(Default)]
pub struct BuildOutputState {
    /// Build log lines (stdout + stderr from gradlew).
    pub lines: Vec<String>,
    /// Whether the build is currently running.
    pub running: bool,
    /// Whether the build completed successfully.
    pub success: Option<bool>,
    /// Set to true when Run is clicked; TerminalView poll reads and clears it to auto-open the panel.
    pub should_open_panel: bool,
}

/// Global build output state so both LeftPanel (Run btn) and TerminalView (BuildView) can access it.
pub static BUILD_STATE: LazyLock<Arc<Mutex<BuildOutputState>>> =
    LazyLock::new(|| Arc::new(Mutex::new(BuildOutputState::default())));
