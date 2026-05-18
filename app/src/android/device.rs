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

    /// Lists all connected devices with basic information.
    pub fn list_devices(&mut self) -> Result<Vec<AndroidDevice>, String> {
        let devices = self
            .server
            .devices()
            .map_err(|e| format!("Failed to list ADB devices: {e}"))?;

        Ok(devices
            .into_iter()
            .map(|d| AndroidDevice {
                serial: d.identifier,
                state: d.state,
                product: None,
                model: None,
                transport_id: None,
            })
            .collect())
    }

    /// Lists all connected devices with extended information (product, model).
    pub fn list_devices_detailed(&mut self) -> Result<Vec<AndroidDevice>, String> {
        let devices = self
            .server
            .devices_long()
            .map_err(|e| format!("Failed to list ADB devices: {e}"))?;

        Ok(devices
            .into_iter()
            .map(|d| AndroidDevice {
                serial: d.identifier,
                state: d.state,
                product: d.product,
                model: d.model,
                transport_id: d.transport_id,
            })
            .collect())
    }

    /// Checks if the ADB server is reachable and at least one device is connected.
    pub fn has_devices(&mut self) -> Result<bool, String> {
        let devices = self.list_devices()?;
        Ok(!devices.is_empty())
    }
}
