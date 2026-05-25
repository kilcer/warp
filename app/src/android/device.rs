use std::process::Command;

use adb_client::server::ADBServer;

/// Represents a connected Android device.
#[derive(Debug, Clone)]
pub struct AndroidDevice {
    /// Device serial number / identifier (e.g., "emulator-5554")
    pub serial: String,
    /// Device state (e.g., "device", "offline", "unauthorized")
    pub state: String,
    /// Product name (e.g., "sailfish")
    pub product: Option<String>,
    /// Model name (e.g., "Pixel")
    pub model: Option<String>,
    /// Transport ID (for USB/TCP identification)
    pub transport_id: Option<u32>,
}

/// Service for interacting with the ADB server and managing devices.
pub struct AdbDeviceService {
    server: ADBServer,
}

impl AdbDeviceService {
    /// Creates a service connected to the default ADB server address (127.0.0.1:5037).
    pub fn default() -> Result<Self, String> {
        let server = ADBServer::default();
        Ok(Self { server })
    }

    /// Checks if the `adb` binary is available in the system PATH.
    pub fn is_adb_available() -> bool {
        Command::new("adb")
            .arg("version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Lists all connected devices by running `adb devices -l` and parsing the
    /// output. This is more reliable than the adb_client TCP crate because it
    /// uses the same CLI that users manually invoke.
    pub fn list_devices_cli() -> Result<Vec<AndroidDevice>, String> {
        let output = Command::new("adb")
            .args(["devices", "-l"])
            .output()
            .map_err(|e| format!("Failed to run adb: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut devices = Vec::new();

        for line in stdout.lines().skip(1) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }

            let serial = parts[0].to_string();
            let state = parts[1].to_string();

            let mut product = None;
            let mut model = None;

            for part in &parts[2..] {
                if let Some(val) = part.strip_prefix("product:") {
                    product = Some(val.to_string());
                } else if let Some(val) = part.strip_prefix("model:") {
                    model = Some(val.to_string());
                }
            }

            devices.push(AndroidDevice {
                serial,
                state,
                product,
                model,
                transport_id: None,
            });
        }

        Ok(devices)
    }

    /// Checks if the ADB server is reachable and at least one device is connected.
    pub fn has_devices() -> Result<bool, String> {
        let devices = Self::list_devices_cli()?;
        Ok(!devices.is_empty())
    }
}
