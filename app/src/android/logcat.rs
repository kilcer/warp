use std::io::{BufRead, BufReader};
use std::process::Stdio;

use super::device::new_command;

/// Log level from Android's logcat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Verbose,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    /// Returns the single-character log level identifier.
    pub fn as_char(&self) -> char {
        match self {
            LogLevel::Verbose => 'V',
            LogLevel::Debug => 'D',
            LogLevel::Info => 'I',
            LogLevel::Warn => 'W',
            LogLevel::Error => 'E',
            LogLevel::Fatal => 'F',
        }
    }

    /// Parses a log level from the single-character identifier.
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'V' => Some(LogLevel::Verbose),
            'D' => Some(LogLevel::Debug),
            'I' => Some(LogLevel::Info),
            'W' => Some(LogLevel::Warn),
            'E' => Some(LogLevel::Error),
            'F' => Some(LogLevel::Fatal),
            _ => None,
        }
    }
}

/// A parsed logcat entry matching Android Studio's format.
#[derive(Debug, Clone)]
pub struct LogcatEntry {
    pub timestamp: String,
    pub pid: u32,
    pub tid: u32,
    pub level: LogLevel,
    pub tag: String,
    pub message: String,
}

/// Parses a raw logcat line into a LogcatEntry.
///
/// Expected format (Android logcat -v threadtime):
/// `MM-DD HH:MM:SS.mmm  PID  TID  L TAG: message`
pub fn parse_logcat_line(line: &str) -> Option<LogcatEntry> {
    // Logcat -v threadtime format: "05-16 10:30:45.123  1234  5678 D TagName: message"
    let parts: Vec<&str> = line.splitn(2, ':').collect();
    if parts.len() < 2 {
        return None;
    }

    let header = parts[0];
    let message = parts[1].trim().to_string();

    // Parse header: "MM-DD HH:MM:SS.mmm  PID  TID  L TAG"
    let header_parts: Vec<&str> = header.split_whitespace().collect();
    if header_parts.len() < 6 {
        return None;
    }

    let date = header_parts[0];
    let time = header_parts[1];
    let pid: u32 = header_parts[2].parse().ok()?;
    let tid: u32 = header_parts[3].parse().ok()?;
    let level = LogLevel::from_char(header_parts[4].chars().next()?)?;
    let tag = header_parts[5].to_string();

    Some(LogcatEntry {
        timestamp: format!("{date} {time}"),
        pid,
        tid,
        level,
        tag,
        message,
    })
}

/// Configuration for logcat streaming.
pub struct LogcatConfig {
    /// Device serial to connect to (or None for default).
    pub device_serial: Option<String>,
    /// Log levels to include. If empty, all levels are shown.
    pub levels: Vec<LogLevel>,
    /// Tag filter regex pattern.
    pub tag_filter: Option<String>,
    /// Message text filter.
    pub text_filter: Option<String>,
    /// Clear the log buffer before starting.
    pub clear_first: bool,
}

impl Default for LogcatConfig {
    fn default() -> Self {
        Self {
            device_serial: None,
            levels: vec![],
            tag_filter: None,
            text_filter: None,
            clear_first: true,
        }
    }
}

/// Starts streaming logcat output, calling the provided callback for each parsed entry.
///
/// **Note**: This is a blocking function. In Warp's UI context, it must be
/// spawned on a dedicated thread to avoid freezing the render loop.
/// A future async/tokio version should be preferred for production use.
pub fn stream_logcat(
    config: &LogcatConfig,
    mut on_entry: impl FnMut(LogcatEntry),
) -> Result<(), String> {
    // Optionally clear the log buffer first
    if config.clear_first {
        let mut clear_cmd = new_command("adb");
        if let Some(ref serial) = config.device_serial {
            clear_cmd.args(["-s", serial]);
        }
        clear_cmd.args(["logcat", "-c"]);
        let _ = clear_cmd.output();
    }

    let mut cmd = new_command("adb");
    if let Some(ref serial) = config.device_serial {
        cmd.args(["-s", serial]);
    }
    cmd.args(["logcat", "-v", "threadtime"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start adb logcat: {e}"))?;

    let stdout = child.stdout.take().ok_or("No stdout from adb logcat")?;
    let reader = BufReader::new(stdout);

    for line in reader.lines() {
        match line {
            Ok(line) => {
                if let Some(entry) = parse_logcat_line(&line) {
                    // Apply filters
                    if !config.levels.is_empty()
                        && !config.levels.contains(&entry.level)
                    {
                        continue;
                    }
                    if let Some(ref tag_filter) = config.tag_filter {
                        if !entry.tag.to_lowercase().contains(&tag_filter.to_lowercase()) {
                            continue;
                        }
                    }
                    if let Some(ref text_filter) = config.text_filter {
                        if !entry
                            .message
                            .to_lowercase()
                            .contains(&text_filter.to_lowercase())
                        {
                            continue;
                        }
                    }
                    on_entry(entry);
                }
            }
            Err(_) => break,
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "logcat_tests.rs"]
mod tests;
