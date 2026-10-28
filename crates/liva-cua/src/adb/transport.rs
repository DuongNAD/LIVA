//! Decoupled transport layer for Android Debug Bridge (ADB) operations.
//!
//! Provides an abstract `AdbTransport` trait, a high-fidelity in-memory `MockAdbTransport`
//! for deterministic headless testing without real hardware, and a `ProcessAdbTransport`
//! executing live commands against physical Android devices or emulators.

use crate::types::CuaError;
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::Duration;

/// Abstract transport trait for issuing raw commands to the Android Debug Bridge (ADB).
#[async_trait]
pub trait AdbTransport: Send + Sync {
    /// Execute an ADB command with an optional target device serial number.
    ///
    /// # Arguments
    /// * `serial` - Optional device serial (e.g., `"emulator-5554"`, `"RFCW300XXXX"`).
    ///              When `None`, the default or only connected device is addressed.
    /// * `args` - Array of command-line arguments to pass to `adb` (e.g. `["shell", "input", "tap", "500", "800"]`).
    async fn execute(&self, serial: Option<&str>, args: &[&str]) -> Result<String, CuaError>;
}

/// In-memory mock ADB transport for 100% headless unit and integration testing.
pub struct MockAdbTransport {
    exact_responses: Mutex<HashMap<String, Result<String, String>>>,
    prefix_responses: Mutex<Vec<(String, Result<String, String>)>>,
    default_response: Mutex<Option<Result<String, String>>>,
    dispatched_commands: Mutex<Vec<(Option<String>, Vec<String>)>>,
}

impl Default for MockAdbTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl MockAdbTransport {
    /// Creates a new empty `MockAdbTransport`.
    pub fn new() -> Self {
        Self {
            exact_responses: Mutex::new(HashMap::new()),
            prefix_responses: Mutex::new(Vec::new()),
            default_response: Mutex::new(None),
            dispatched_commands: Mutex::new(Vec::new()),
        }
    }

    /// Sets an exact match response for a command string (e.g., `"devices -l"` or `"shell input tap 100 200"`).
    pub fn set_response(&self, command: impl Into<String>, output: impl Into<String>) {
        self.exact_responses
            .lock()
            .insert(command.into(), Ok(output.into()));
    }

    /// Sets an exact match error for a command string.
    pub fn set_error(&self, command: impl Into<String>, error: impl Into<String>) {
        self.exact_responses
            .lock()
            .insert(command.into(), Err(error.into()));
    }

    /// Sets a prefix match response (e.g., prefix `"shell input text"`).
    pub fn set_prefix_response(&self, prefix: impl Into<String>, output: impl Into<String>) {
        self.prefix_responses
            .lock()
            .push((prefix.into(), Ok(output.into())));
    }

    /// Sets a prefix match error.
    pub fn set_prefix_error(&self, prefix: impl Into<String>, error: impl Into<String>) {
        self.prefix_responses
            .lock()
            .push((prefix.into(), Err(error.into())));
    }

    /// Sets the fallback default response returned when no exact or prefix match is found.
    pub fn set_default_response(&self, output: impl Into<String>) {
        *self.default_response.lock() = Some(Ok(output.into()));
    }

    /// Retrieves all dispatched command tuples `(serial, args)`.
    pub fn dispatched_commands(&self) -> Vec<(Option<String>, Vec<String>)> {
        self.dispatched_commands.lock().clone()
    }

    /// Returns the most recently dispatched command tuple, if any.
    pub fn last_dispatched(&self) -> Option<(Option<String>, Vec<String>)> {
        self.dispatched_commands.lock().last().cloned()
    }

    /// Returns the total number of commands dispatched through this transport.
    pub fn count_dispatched(&self) -> usize {
        self.dispatched_commands.lock().len()
    }

    /// Clears the history of dispatched commands.
    pub fn clear_dispatched(&self) {
        self.dispatched_commands.lock().clear();
    }
}

#[async_trait]
impl AdbTransport for MockAdbTransport {
    async fn execute(&self, serial: Option<&str>, args: &[&str]) -> Result<String, CuaError> {
        let args_vec: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let cmd_line = args.join(" ");

        // Record invocation
        self.dispatched_commands
            .lock()
            .push((serial.map(|s| s.to_string()), args_vec));

        // 1. Check exact match
        if let Some(res) = self.exact_responses.lock().get(&cmd_line) {
            return match res {
                Ok(out) => Ok(out.clone()),
                Err(err) => Err(CuaError::Internal(err.clone())),
            };
        }

        // 1b. Check exact match with -s <serial> prefix if provided
        if let Some(ser) = serial {
            let full_cmd = format!("-s {} {}", ser, cmd_line);
            if let Some(res) = self.exact_responses.lock().get(&full_cmd) {
                return match res {
                    Ok(out) => Ok(out.clone()),
                    Err(err) => Err(CuaError::Internal(err.clone())),
                };
            }
        }

        // 2. Check prefix matches (in reverse order so later registered rules take precedence)
        let prefixes = self.prefix_responses.lock();
        for (prefix, res) in prefixes.iter().rev() {
            if cmd_line.starts_with(prefix) {
                return match res {
                    Ok(out) => Ok(out.clone()),
                    Err(err) => Err(CuaError::Internal(err.clone())),
                };
            }
        }

        // 3. Fallback to default response if configured
        if let Some(ref res) = *self.default_response.lock() {
            return match res {
                Ok(out) => Ok(out.clone()),
                Err(err) => Err(CuaError::Internal(err.clone())),
            };
        }

        // 4. Default empty output for unmatched commands (e.g. input tap, input keyevent)
        Ok(String::new())
    }
}

/// Live OS process-based ADB transport invoking the `adb` executable.
pub struct ProcessAdbTransport {
    adb_path: String,
    timeout_duration: Duration,
}

impl ProcessAdbTransport {
    /// Creates a new `ProcessAdbTransport` with a custom path and timeout in milliseconds.
    pub fn new(adb_path: impl Into<String>, timeout_ms: u64) -> Self {
        Self {
            adb_path: adb_path.into(),
            timeout_duration: Duration::from_millis(timeout_ms),
        }
    }
}

impl Default for ProcessAdbTransport {
    fn default() -> Self {
        Self::new("adb", 5000)
    }
}

#[async_trait]
impl AdbTransport for ProcessAdbTransport {
    async fn execute(&self, serial: Option<&str>, args: &[&str]) -> Result<String, CuaError> {
        let mut cmd = tokio::process::Command::new(&self.adb_path);

        if let Some(s) = serial {
            cmd.arg("-s").arg(s);
        }

        cmd.args(args);

        // Windows process flag to avoid popping up console window
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let run_future = cmd.output();

        let output = match tokio::time::timeout(self.timeout_duration, run_future).await {
            Ok(Ok(out)) => out,
            Ok(Err(io_err)) => {
                return Err(CuaError::Internal(format!(
                    "Failed to execute adb process at '{}': {}",
                    self.adb_path, io_err
                )));
            }
            Err(_) => {
                return Err(CuaError::ActionTimeout(
                    self.timeout_duration.as_millis() as u64
                ));
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let msg = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("Exit code: {:?}", output.status.code())
            };
            return Err(CuaError::Internal(format!("adb command failed: {}", msg)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}
