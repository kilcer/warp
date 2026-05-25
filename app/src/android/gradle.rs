use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Result of a Gradle command execution.
#[derive(Debug)]
pub struct GradleResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

/// Service for executing Gradle commands in an Android project.
pub struct GradleService {
    project_dir: PathBuf,
}

impl GradleService {
    /// Creates a new Gradle service for the given project directory.
    pub fn new(project_dir: PathBuf) -> Self {
        Self { project_dir }
    }

    /// Detects the gradle wrapper script (gradlew or gradlew.bat on Windows).
    fn gradlew_path(&self) -> PathBuf {
        #[cfg(windows)]
        {
            self.project_dir.join("gradlew.bat")
        }
        #[cfg(not(windows))]
        {
            self.project_dir.join("gradlew")
        }
    }

    /// Checks if gradlew exists in the project directory.
    pub fn has_gradlew(&self) -> bool {
        self.gradlew_path().exists()
    }

    /// Executes a Gradle command and returns the result.
    fn execute(&self, args: &[&str]) -> Result<GradleResult, String> {
        let gradlew = self.gradlew_path();

        if !gradlew.exists() {
            return Err(format!(
                "gradlew not found at {}. Make sure this is an Android project.",
                gradlew.display()
            ));
        }

        // Ensure gradlew is executable (common issue after git clone on Linux).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&gradlew)
                .map_err(|e| format!("Cannot stat gradlew: {e}"))?
                .permissions();
            if perms.mode() & 0o111 == 0 {
                std::fs::set_permissions(&gradlew, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| format!("Cannot chmod +x gradlew: {e}"))?;
            }
        }

        let output = Command::new(&gradlew)
            .args(args)
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| format!("Failed to execute gradlew: {e}"))?;

        Ok(GradleResult {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code(),
        })
    }

    /// Runs `./gradlew assembleDebug`
    pub fn assemble_debug(&self) -> Result<GradleResult, String> {
        self.execute(&["assembleDebug"])
    }

    /// Executes a Gradle command with streaming output, calling `on_line` for
    /// each line of stdout/stderr as it is produced (from a background thread).
    /// This is the streaming counterpart to [`Self::execute`] — use it when you
    /// need progressive output display (e.g. in a build output panel).
    pub fn execute_streaming(
        &self,
        args: &[&str],
        on_line: impl FnMut(String) + Send + 'static,
    ) -> Result<GradleResult, String> {
        let gradlew = self.gradlew_path();

        if !gradlew.exists() {
            return Err(format!(
                "gradlew not found at {}. Make sure this is an Android project.",
                gradlew.display()
            ));
        }

        // Ensure gradlew is executable (common issue after git clone on Linux).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&gradlew)
                .map_err(|e| format!("Cannot stat gradlew: {e}"))?
                .permissions();
            if perms.mode() & 0o111 == 0 {
                std::fs::set_permissions(&gradlew, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| format!("Cannot chmod +x gradlew: {e}"))?;
            }
        }

        let mut child = Command::new(&gradlew)
            .args(args)
            .current_dir(&self.project_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to execute gradlew: {e}"))?;

        let stdout = child.stdout.take().expect("No stdout from gradlew");
        let stderr = child.stderr.take().expect("No stderr from gradlew");

        // Wrap the callback in Arc<Mutex> so both reader threads can call it.
        let on_line = std::sync::Arc::new(std::sync::Mutex::new(on_line));

        let on_line_stdout = std::sync::Arc::clone(&on_line);
        let stdout_handle = std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            let mut lines = Vec::new();
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        on_line_stdout.lock().unwrap()(l.clone());
                        lines.push(l);
                    }
                    Err(e) => {
                        on_line_stdout.lock().unwrap()(format!("[read error] {e}"));
                    }
                }
            }
            lines
        });

        let on_line_stderr = std::sync::Arc::clone(&on_line);
        let stderr_handle = std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            let mut lines = Vec::new();
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        on_line_stderr.lock().unwrap()(format!("[stderr] {l}"));
                        lines.push(l);
                    }
                    Err(e) => {
                        on_line_stderr.lock().unwrap()(format!("[read error] {e}"));
                    }
                }
            }
            lines
        });

        let stdout_lines = stdout_handle.join().unwrap_or_default();
        let stderr_lines = stderr_handle.join().unwrap_or_default();

        let status = child
            .wait()
            .map_err(|e| format!("Failed to wait on gradlew: {e}"))?;

        Ok(GradleResult {
            success: status.success(),
            stdout: stdout_lines.join("\n"),
            stderr: stderr_lines.join("\n"),
            exit_code: status.code(),
        })
    }

    /// Streaming version of `assemble_debug`.
    pub fn assemble_debug_streaming(
        &self,
        on_line: impl FnMut(String) + Send + 'static,
    ) -> Result<GradleResult, String> {
        self.execute_streaming(&["assembleDebug"], on_line)
    }

    /// Runs `./gradlew assembleRelease` (reserved for future use).
    #[allow(dead_code)]
    pub fn assemble_release(&self) -> Result<GradleResult, String> {
        self.execute(&["assembleRelease"])
    }

    /// Runs `./gradlew build`
    pub fn build(&self) -> Result<GradleResult, String> {
        self.execute(&["build"])
    }

    /// Runs `./gradlew clean`
    pub fn clean(&self) -> Result<GradleResult, String> {
        self.execute(&["clean"])
    }

    /// Returns the project directory path.
    pub fn project_dir(&self) -> &Path {
        &self.project_dir
    }
}
