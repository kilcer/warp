use std::path::{Path, PathBuf};
use std::process::Command;

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
