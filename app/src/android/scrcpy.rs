//! Screen mirroring via scrcpy protocol.
//!
//! Workflow:
//! 1. Push scrcpy-server.jar to device
//! 2. Start server via ADB
//! 3. Establish ADB tunnel (video + control sockets)
//! 4. Receive and decode H.264 video stream
//! 5. Render frames via WarpUI WGPU texture pipeline
//! 6. Forward keyboard/mouse events back to device

use std::io::Read;
use std::net::TcpStream;
use std::process::Child;

use super::device::new_command;

/// Default scrcpy server version used by the protocol crate.
const SCRCPY_SERVER_VERSION: &str = "3.3.3";

/// Path where scrcpy-server.jar is pushed on the device.
const SCRCPY_SERVER_PATH: &str = "/data/local/tmp/scrcpy-server.jar";

/// Maximum video resolution sent by the device.
const DEFAULT_MAX_SIZE: u32 = 1920;

/// Default video bitrate in bits per second.
const DEFAULT_BITRATE: u32 = 8_000_000;

/// Initial wait time for server to boot (milliseconds).
const SERVER_BOOT_POLL_MS: u64 = 100;
/// Maximum wait time for server to boot (milliseconds).
const SERVER_BOOT_TIMEOUT_MS: u64 = 3000;

/// Configuration for a scrcpy mirroring session.
pub struct ScrcpyConfig {
    /// Device serial to mirror.
    pub device_serial: String,
    /// Maximum video dimension (width or height). Default: 1920.
    pub max_size: u32,
    /// Video bitrate in bits per second. Default: 8 Mbps.
    pub bitrate: u32,
    /// Whether to forward audio.
    pub audio: bool,
    /// Local TCP port for the ADB tunnel.
    pub local_port: u16,
}

impl Default for ScrcpyConfig {
    fn default() -> Self {
        Self {
            device_serial: String::new(),
            max_size: DEFAULT_MAX_SIZE,
            bitrate: DEFAULT_BITRATE,
            audio: false,
            local_port: 27183,
        }
    }
}

/// The scrcpy client that manages the mirroring session.
pub struct ScrcpyClient {
    config: ScrcpyConfig,
    video_stream: Option<TcpStream>,
    /// Held so the server process is killed on drop/disconnect.
    server_child: Option<Child>,
}

impl Drop for ScrcpyClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

impl ScrcpyClient {
    /// Creates a new scrcpy client with the given configuration.
    pub fn new(config: ScrcpyConfig) -> Self {
        Self {
            config,
            video_stream: None,
            server_child: None,
        }
    }

    /// Pushes the scrcpy server JAR to the device via ADB.
    pub fn push_server(&self, jar_path: &str) -> Result<(), String> {
        let output = new_command("adb")
            .args([
                "-s",
                &self.config.device_serial,
                "push",
                jar_path,
                SCRCPY_SERVER_PATH,
            ])
            .output()
            .map_err(|e| format!("Failed to push scrcpy server: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("ADB push failed: {stderr}"));
        }
        Ok(())
    }

    /// Starts the scrcpy server on the device.
    ///
    /// The child process handle is stored so it can be killed on disconnect.
    pub fn start_server(&mut self) -> Result<(), String> {
        let args = format!(
            "CLASSPATH={SCRCPY_SERVER_PATH} app_process / com.genymobile.scrcpy.Server \
             {SCRCPY_SERVER_VERSION} \
             tunnel_forward=true max_size={} bit_rate={} \
             audio={} send_frame_meta=true",
            self.config.max_size,
            self.config.bitrate,
            self.config.audio,
        );

        let child = new_command("adb")
            .args(["-s", &self.config.device_serial, "shell", &args])
            .spawn()
            .map_err(|e| format!("Failed to start scrcpy server: {e}"))?;

        let pid = child.id();
        self.server_child = Some(child);
        log::info!("scrcpy server started on device (pid={pid})");

        Ok(())
    }

    /// Sets up the ADB tunnel (forward) for video and control sockets.
    pub fn setup_tunnel(&self) -> Result<(), String> {
        let local_port = self.config.local_port;

        // Remove any existing forward first
        let _ = new_command("adb")
            .args([
                "-s",
                &self.config.device_serial,
                "forward",
                "--remove",
                &format!("tcp:{local_port}"),
            ])
            .output();

        // Set up forward: local:port -> device:scrcpy socket
        let output = new_command("adb")
            .args([
                "-s",
                &self.config.device_serial,
                "forward",
                &format!("tcp:{local_port}"),
                "localabstract:scrcpy",
            ])
            .output()
            .map_err(|e| format!("Failed to set up ADB tunnel: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("ADB forward failed: {stderr}"));
        }

        Ok(())
    }

    /// Connects the video stream (receives H.264 frames from device).
    pub fn connect_video(&mut self) -> Result<(), String> {
        let addr = format!("127.0.0.1:{}", self.config.local_port);
        let stream = TcpStream::connect(&addr)
            .map_err(|e| format!("Failed to connect video stream at {addr}: {e}"))?;

        stream
            .set_nonblocking(true)
            .map_err(|e| format!("Failed to set non-blocking: {e}"))?;

        self.video_stream = Some(stream);
        Ok(())
    }

    /// Reads the next raw video packet from the device.
    ///
    /// Handles non-blocking WouldBlock errors by returning `Ok(0)` (no data).
    /// Returns the raw packet bytes (H.264 NAL units prefixed with scrcpy frame header).
    /// Callers should decode using an H.264 decoder (ffmpeg, VideoToolbox, DXVA2, etc.)
    pub fn read_video_packet(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        let stream = self
            .video_stream
            .as_mut()
            .ok_or("Video stream not connected")?;

        match stream.read(buf) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0),
            Err(e) => Err(format!("Failed to read video packet: {e}")),
        }
    }

    /// Tears down the mirroring session — closes sockets, kills server, cleans up ADB tunnel.
    pub fn disconnect(&mut self) {
        // Close sockets
        self.video_stream = None;

        // Kill the server process
        if let Some(mut child) = self.server_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        // Clean up ADB forward
        let local_port = self.config.local_port;
        let _ = new_command("adb")
            .args([
                "-s",
                &self.config.device_serial,
                "forward",
                "--remove",
                &format!("tcp:{local_port}"),
            ])
            .output();
    }
}

/// Starts a full scrcpy mirroring session.
///
/// This is a convenience function that:
/// 1. Pushes the server JAR (must exist on disk)
/// 2. Starts the server on device
/// 3. Polls until the ADB tunnel is connectable (non-blocking)
/// 4. Connects video stream
///
/// Returns a connected `ScrcpyClient` ready for frame reading.
pub fn start_mirroring(config: ScrcpyConfig, jar_path: &str) -> Result<ScrcpyClient, String> {
    let mut client = ScrcpyClient::new(config);

    client.push_server(jar_path)?;
    client.start_server()?;
    client.setup_tunnel()?;

    // Poll until server is ready (non-blocking per iteration)
    let start = std::time::Instant::now();
    let addr = format!("127.0.0.1:{}", client.config.local_port);
    loop {
        match TcpStream::connect(&addr) {
            Ok(_) => break, // Server is up
            Err(_) => {
                if start.elapsed() > std::time::Duration::from_millis(SERVER_BOOT_TIMEOUT_MS) {
                    client.disconnect();
                    return Err("scrcpy server did not start within timeout".to_string());
                }
                std::thread::sleep(std::time::Duration::from_millis(SERVER_BOOT_POLL_MS));
            }
        }
    }

    client.connect_video()?;
    Ok(client)
}
