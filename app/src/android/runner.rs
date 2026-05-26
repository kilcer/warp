use std::path::{Path, PathBuf};

use super::device::new_command;
use super::gradle::GradleService;

/// Result of parsing an Android app's identity from APK or manifest.
#[derive(Debug, Clone)]
pub struct AppIdentity {
    pub package_name: String,
    pub launch_activity: Option<String>,
}

/// Service that orchestrates the full "Run 'app'" workflow:
/// Gradle assemble → find APK → ADB install → ADB launch.
pub struct AndroidRunService {
    gradle: GradleService,
}

impl AndroidRunService {
    pub fn new(project_dir: PathBuf) -> Self {
        Self {
            gradle: GradleService::new(project_dir),
        }
    }

    /// Runs the full pipeline: assembleDebug → install → launch.
    pub fn run_app(&self, device_serial: &str) -> Result<String, String> {
        let build_result = self.gradle.assemble_debug()?;
        if !build_result.success {
            return Err(format!("Build failed:\n{}", build_result.stderr));
        }

        let apk_path = self.find_apk()?;
        let identity = self.extract_app_identity(&apk_path)?;
        self.install_apk(device_serial, &apk_path)?;
        self.launch_app(device_serial, &identity)?;

        let activity = identity
            .launch_activity
            .as_deref()
            .unwrap_or("(default)");
        Ok(format!(
            "App launched: {}/{} on {device_serial}",
            identity.package_name, activity
        ))
    }

    /// Finds the debug APK in the typical Gradle output location.
    pub fn find_apk(&self) -> Result<PathBuf, String> {
        let apk_dir = self
            .gradle
            .project_dir()
            .join("app")
            .join("build")
            .join("outputs")
            .join("apk")
            .join("debug");

        let apk_path = apk_dir.join("app-debug.apk");
        if apk_path.exists() {
            return Ok(apk_path);
        }

        if let Ok(entries) = std::fs::read_dir(&apk_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "apk") {
                    return Ok(path);
                }
            }
        }

        Err(format!(
            "No APK found at {}. Did the build succeed?",
            apk_dir.display()
        ))
    }

    /// Extracts the app's package name and launchable activity using aapt.
    pub fn extract_app_identity(&self, apk_path: &Path) -> Result<AppIdentity, String> {
        let output = Command::new("aapt")
            .args(["dump", "badging", &apk_path.to_string_lossy()])
            .output()
            .map_err(|e| format!(
                "Failed to run aapt: {e}. Make sure Android SDK build-tools are installed."
            ))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        if !output.status.success() {
            return self.extract_app_identity_aapt2(apk_path);
        }

        self.parse_aapt_badging(&stdout)
    }

    /// Fallback using aapt2 (newer Android SDK versions).
    fn extract_app_identity_aapt2(&self, apk_path: &Path) -> Result<AppIdentity, String> {
        let output = Command::new("aapt2")
            .args(["dump", "badging", &apk_path.to_string_lossy()])
            .output()
            .map_err(|e| format!(
                "Neither aapt nor aapt2 found. Install Android SDK build-tools.\nError: {e}"
            ))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        self.parse_aapt_badging(&stdout)
    }

    /// Parses aapt badging output for package name and launchable activity.
    fn parse_aapt_badging(&self, output: &str) -> Result<AppIdentity, String> {
        let mut package_name = None;
        let mut launch_activity = None;

        for line in output.lines() {
            if let Some(name) = Self::extract_quoted_value(line, "package: name=") {
                package_name = Some(name);
            }
            if let Some(name) = Self::extract_quoted_value(line, "launchable-activity: name=") {
                launch_activity = Some(name);
            }
        }

        let package_name = package_name
            .ok_or_else(|| "Could not find package name in aapt output".to_string())?;

        Ok(AppIdentity {
            package_name,
            launch_activity,
        })
    }

    /// Extracts a single-quoted value from a key=value pair in aapt output.
    /// Example: `package: name='com.example.app'` → `Some("com.example.app")`
    fn extract_quoted_value(line: &str, prefix: &str) -> Option<String> {
        let rest = line.strip_prefix(prefix)?;
        let value = rest.trim_start_matches('\'').split('\'').next()?;
        Some(value.to_string())
    }

    /// Installs the APK to the specified device.
    pub fn install_apk(&self, device_serial: &str, apk_path: &Path) -> Result<(), String> {
        let output = new_command("adb")
            .args([
                "-s",
                device_serial,
                "install",
                "-r",
                &apk_path.to_string_lossy(),
            ])
            .output()
            .map_err(|e| format!("Failed to run adb install: {e}"))?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("ADB install failed:\nstdout: {stdout}\nstderr: {stderr}"));
        }

        Ok(())
    }

    /// Launches the app on the specified device using `am start`.
    pub fn launch_app(&self, device_serial: &str, identity: &AppIdentity) -> Result<(), String> {
        let component = match &identity.launch_activity {
            Some(activity) if activity.starts_with('.') => {
                // ".MainActivity" → "com.example.app/.MainActivity"
                format!("{}/{}", &identity.package_name, activity)
            }
            Some(activity) if activity.contains('.') => {
                // "com.example.MainActivity" → full qualified
                format!("{}/{}", &identity.package_name, activity)
            }
            Some(activity) => {
                // "MainActivity" → "com.example.app/.MainActivity"
                format!("{}/.{}", &identity.package_name, activity)
            }
            None => identity.package_name.clone(),
        };

        let output = new_command("adb")
            .args([
                "-s",
                device_serial,
                "shell",
                "am",
                "start",
                "-n",
                &component,
            ])
            .output()
            .map_err(|e| format!("Failed to launch app: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to launch app {component}: {stderr}"));
        }

        Ok(())
    }

    /// Returns the Gradle service for standalone use.
    pub fn gradle(&self) -> &GradleService {
        &self.gradle
    }
}

/// Reads the package name (namespace) from the Android project.
/// Tries in order:
/// 1. `app/build.gradle` → `namespace 'com.xxx'` or `namespace "com.xxx"`
/// 2. `app/build.gradle.kts` → `namespace = "com.xxx"`
/// 3. `app/src/main/AndroidManifest.xml` → `package="com.xxx"` (older AGP)
pub fn read_package_name_from_manifest(project_dir: &Path) -> Option<String> {
    // Try build.gradle (Groovy)
    if let Some(ns) = read_namespace_from_gradle(&project_dir.join("app/build.gradle")) {
        return Some(ns);
    }
    // Try build.gradle.kts (Kotlin DSL)
    if let Some(ns) = read_namespace_from_gradle_kts(&project_dir.join("app/build.gradle.kts")) {
        return Some(ns);
    }
    // Fallback: AndroidManifest.xml (older projects)
    read_package_from_android_manifest(project_dir)
}

/// Reads `namespace` from a Groovy build.gradle file.
fn read_namespace_from_gradle(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    extract_namespace(&content)
}

/// Reads `namespace` from a Kotlin DSL build.gradle.kts file.
fn read_namespace_from_gradle_kts(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    extract_namespace(&content)
}

/// Extracts namespace value from gradle file content.
fn extract_namespace(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        // Groovy: namespace 'com.xxx' or namespace "com.xxx"
        // Kotlin: namespace = "com.xxx" or namespace = "com.xxx"
        if !trimmed.starts_with("namespace") {
            continue;
        }
        let rest = trimmed.strip_prefix("namespace")?.trim();
        // Strip leading = for Kotlin DSL
        let rest = rest.strip_prefix('=').unwrap_or(rest).trim();
        // Extract quoted value
        if let Some(v) = rest.strip_prefix('"') {
            if let Some(end) = v.find('"') {
                let val = &v[..end];
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        } else if let Some(v) = rest.strip_prefix('\'') {
            if let Some(end) = v.find('\'') {
                let val = &v[..end];
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// Reads `package="..."` from AndroidManifest.xml (older AGP).
fn read_package_from_android_manifest(project_dir: &Path) -> Option<String> {
    let manifest_path = project_dir.join("app/src/main/AndroidManifest.xml");
    let content = std::fs::read_to_string(&manifest_path).ok()?;
    for line in content.lines() {
        if !line.contains("<manifest") {
            continue;
        }
        for part in line.split_whitespace() {
            if let Some(rest) = part.strip_prefix("package=") {
                let value = rest.trim_matches('"').trim_matches('\'');
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

// ========== Test helpers ==========
// These expose private methods for unit testing pure parsing logic.

impl AndroidRunService {
    /// Test-only wrapper for `parse_aapt_badging`.
    #[doc(hidden)]
    pub fn test_parse_aapt_badging(output: &str) -> Result<AppIdentity, String> {
        // Create a dummy instance to access the method.
        let dummy = AndroidRunService::new(std::path::PathBuf::from("."));
        dummy.parse_aapt_badging(output)
    }

    /// Test-only wrapper for `extract_quoted_value`.
    #[doc(hidden)]
    pub fn extract_quoted_value_test(line: &str, prefix: &str) -> Option<String> {
        Self::extract_quoted_value(line, prefix)
    }
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
