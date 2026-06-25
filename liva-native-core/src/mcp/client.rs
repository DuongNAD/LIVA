use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tracing::{info};

pub struct ProcessWrapper {
    child: Child,
}

impl ProcessWrapper {
    pub fn spawn(command: &str, args: &[&str]) -> Result<Self, String> {
        let child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn {}: {}", command, e))?;
        
        info!("Spawned external MCP server: {}", command);
        Ok(Self { child })
    }

    pub async fn send_request(&mut self, payload: &str) -> Result<(), String> {
        if let Some(stdin) = self.child.stdin.as_mut() {
            let mut data = payload.to_string();
            data.push('\n');
            stdin.write_all(data.as_bytes()).await.map_err(|e| e.to_string())?;
            stdin.flush().await.map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err("No stdin attached".to_string())
        }
    }

    pub async fn read_response(&mut self) -> Result<String, String> {
        if let Some(stdout) = self.child.stdout.as_mut() {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) => Err("EOF".to_string()),
                Ok(_) => Ok(line.trim().to_string()),
                Err(e) => Err(e.to_string()),
            }
        } else {
            Err("No stdout attached".to_string())
        }
    }
}
