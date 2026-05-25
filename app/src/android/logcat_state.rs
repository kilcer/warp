use std::sync::{Arc, LazyLock, Mutex};

use super::device::AndroidDevice;

/// Logcat output state shared between the background logcat thread and the UI.
#[derive(Default)]
pub struct LogcatOutputState {
    /// Raw logcat output lines.
    pub entries: Vec<String>,
    /// Whether logcat streaming is active.
    pub running: bool,
    /// Connected Android devices (updated by USB watcher thread).
    pub devices: Vec<AndroidDevice>,
    /// Serial of the device currently selected for logcat streaming.
    pub selected_serial: Option<String>,
    /// Package name of the current project (used for PID-based logcat filtering).
    pub package_name: Option<String>,
}

/// Global logcat state so both the background thread and LogcatView can access it.
pub static LOGCAT_STATE: LazyLock<Arc<Mutex<LogcatOutputState>>> =
    LazyLock::new(|| Arc::new(Mutex::new(LogcatOutputState::default())));
