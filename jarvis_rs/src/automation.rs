use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub name: String,
    pub arguments: Value,
    #[serde(default = "default_risk_level")]
    pub risk: RiskLevel,
    #[serde(default)]
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub name: String,
    pub success: bool,
    pub summary: String,
    pub output: Option<String>,
    pub artifact_path: Option<PathBuf>,
}

fn default_risk_level() -> RiskLevel {
    RiskLevel::Low
}

#[async_trait]
pub trait AutomationBackend: Send + Sync {
    async fn call_tool(&self, request: ToolCallRequest) -> Result<ToolCallResult>;
    fn classify_risk(&self, request: &ToolCallRequest) -> RiskLevel;
    fn available_tools(&self) -> &'static [&'static str];
}

#[derive(Debug)]
pub struct LocalAutomationBackend {
    allowed_paths: Vec<PathBuf>,
}

impl LocalAutomationBackend {
    pub fn new(allowed_paths: Vec<PathBuf>) -> Self {
        Self { allowed_paths }
    }

    fn shell_program() -> &'static str {
        "/bin/sh"
    }

    fn shell_args(script: &str) -> [&str; 2] {
        ["-lc", script]
    }

    fn is_path_allowed(&self, path: &Path) -> bool {
        self.allowed_paths.iter().any(|allowed| path.starts_with(allowed))
    }

    fn summary(name: &str, summary: impl Into<String>, output: Option<String>) -> ToolCallResult {
        ToolCallResult {
            name: name.to_string(),
            success: true,
            summary: summary.into(),
            output,
            artifact_path: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RunShellArgs {
    command: String,
}

#[derive(Debug, Deserialize)]
struct OpenAppArgs {
    app_name: String,
}

#[derive(Debug, Deserialize)]
struct PathArgs {
    path: String,
}

#[derive(Debug, Deserialize)]
struct WriteFileArgs {
    path: String,
    contents: String,
}

#[derive(Debug, Deserialize)]
struct MovePathArgs {
    from: String,
    to: String,
}

#[derive(Debug, Deserialize)]
struct BrowserArgs {
    url: String,
}

#[derive(Debug, Deserialize)]
struct AppleScriptArgs {
    script: String,
}

#[derive(Debug, Deserialize)]
struct ScreenshotArgs {
    path: String,
}

#[async_trait]
impl AutomationBackend for LocalAutomationBackend {
    async fn call_tool(&self, request: ToolCallRequest) -> Result<ToolCallResult> {
        match request.name.as_str() {
            "run_shell" => {
                let args: RunShellArgs = serde_json::from_value(request.arguments)?;
                let output = Command::new(Self::shell_program())
                    .args(Self::shell_args(&args.command))
                    .output()
                    .with_context(|| format!("failed to run shell command: {}", args.command))?;
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let combined = if stderr.is_empty() {
                    stdout.clone()
                } else if stdout.is_empty() {
                    stderr.clone()
                } else {
                    format!("{}\n{}", stdout, stderr)
                };
                if !output.status.success() {
                    bail!("shell command failed: {}", combined);
                }
                Ok(Self::summary("run_shell", "Shell command completed", Some(combined)))
            }
            "open_app" => {
                let args: OpenAppArgs = serde_json::from_value(request.arguments)?;
                #[cfg(target_os = "macos")]
                let mut command = {
                    let mut command = Command::new("open");
                    command.arg("-a").arg(&args.app_name);
                    command
                };
                #[cfg(not(target_os = "macos"))]
                let mut command = {
                    let mut command = Command::new("xdg-open");
                    command.arg(&args.app_name);
                    command
                };
                command.status().with_context(|| format!("failed to open {}", args.app_name))?;
                Ok(Self::summary(
                    "open_app",
                    format!("Opened {}", args.app_name),
                    None,
                ))
            }
            "list_dir" => {
                let args: PathArgs = serde_json::from_value(request.arguments)?;
                let entries = fs::read_dir(&args.path)
                    .with_context(|| format!("failed to list {}", args.path))?
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.file_name().to_string_lossy().to_string())
                    .collect::<Vec<_>>();
                Ok(Self::summary(
                    "list_dir",
                    format!("Listed {}", args.path),
                    Some(entries.join("\n")),
                ))
            }
            "create_folder" => {
                let args: PathArgs = serde_json::from_value(request.arguments)?;
                fs::create_dir_all(&args.path)
                    .with_context(|| format!("failed to create {}", args.path))?;
                Ok(Self::summary(
                    "create_folder",
                    format!("Created folder {}", args.path),
                    None,
                ))
            }
            "read_file" => {
                let args: PathArgs = serde_json::from_value(request.arguments)?;
                let contents =
                    fs::read_to_string(&args.path).with_context(|| format!("failed to read {}", args.path))?;
                Ok(Self::summary(
                    "read_file",
                    format!("Read {}", args.path),
                    Some(contents),
                ))
            }
            "write_file" => {
                let args: WriteFileArgs = serde_json::from_value(request.arguments)?;
                fs::write(&args.path, args.contents)
                    .with_context(|| format!("failed to write {}", args.path))?;
                Ok(Self::summary(
                    "write_file",
                    format!("Wrote {}", args.path),
                    None,
                ))
            }
            "move_path" => {
                let args: MovePathArgs = serde_json::from_value(request.arguments)?;
                fs::rename(&args.from, &args.to)
                    .with_context(|| format!("failed to move {} to {}", args.from, args.to))?;
                Ok(Self::summary(
                    "move_path",
                    format!("Moved {} to {}", args.from, args.to),
                    None,
                ))
            }
            "delete_path" => {
                let args: PathArgs = serde_json::from_value(request.arguments)?;
                let path = PathBuf::from(&args.path);
                if path.is_dir() {
                    fs::remove_dir_all(&path)
                        .with_context(|| format!("failed to remove directory {}", args.path))?;
                } else {
                    fs::remove_file(&path)
                        .with_context(|| format!("failed to remove file {}", args.path))?;
                }
                Ok(Self::summary(
                    "delete_path",
                    format!("Deleted {}", args.path),
                    None,
                ))
            }
            "browser_open" => {
                let args: BrowserArgs = serde_json::from_value(request.arguments)?;
                #[cfg(target_os = "macos")]
                let status = Command::new("open").arg(&args.url).status();
                #[cfg(not(target_os = "macos"))]
                let status = Command::new("xdg-open").arg(&args.url).status();
                status.with_context(|| format!("failed to open browser for {}", args.url))?;
                Ok(Self::summary(
                    "browser_open",
                    format!("Opened {}", args.url),
                    None,
                ))
            }
            "browser_extract" => {
                let args: BrowserArgs = serde_json::from_value(request.arguments)?;
                let output = Command::new(Self::shell_program())
                    .args(Self::shell_args(&format!("curl -L --silent '{}'", args.url)))
                    .output()
                    .with_context(|| format!("failed to fetch {}", args.url))?;
                if !output.status.success() {
                    bail!("browser_extract failed for {}", args.url);
                }
                let contents = String::from_utf8_lossy(&output.stdout).to_string();
                Ok(Self::summary(
                    "browser_extract",
                    format!("Fetched {}", args.url),
                    Some(contents),
                ))
            }
            "applescript_run" => {
                let args: AppleScriptArgs = serde_json::from_value(request.arguments)?;
                #[cfg(target_os = "macos")]
                {
                    let output = Command::new("osascript")
                        .arg("-e")
                        .arg(&args.script)
                        .output()
                        .context("failed to execute osascript")?;
                    let contents = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !output.status.success() {
                        bail!(
                            "AppleScript failed: {}",
                            String::from_utf8_lossy(&output.stderr).trim()
                        );
                    }
                    Ok(Self::summary(
                        "applescript_run",
                        "AppleScript completed",
                        Some(contents),
                    ))
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = args;
                    bail!("AppleScript is only available on macOS");
                }
            }
            "take_screenshot" => {
                let args: ScreenshotArgs = serde_json::from_value(request.arguments)?;
                #[cfg(target_os = "macos")]
                let status = Command::new("screencapture").arg("-x").arg(&args.path).status();
                #[cfg(not(target_os = "macos"))]
                let status = Command::new(Self::shell_program())
                    .args(Self::shell_args(&format!(
                        "which gnome-screenshot >/dev/null && gnome-screenshot -f '{}'",
                        args.path
                    )))
                    .status();
                status.with_context(|| format!("failed to capture screenshot to {}", args.path))?;
                Ok(ToolCallResult {
                    name: "take_screenshot".to_string(),
                    success: true,
                    summary: format!("Captured screenshot to {}", args.path),
                    output: None,
                    artifact_path: Some(PathBuf::from(args.path)),
                })
            }
            "send_sms" => Ok(Self::summary(
                "send_sms",
                "SMS dispatch is handled by the background SMS service",
                None,
            )),
            other => Err(anyhow!("unsupported tool: {}", other)),
        }
    }

    fn classify_risk(&self, request: &ToolCallRequest) -> RiskLevel {
        match request.name.as_str() {
            "delete_path" => RiskLevel::High,
            "write_file" | "move_path" => {
                let path = request
                    .arguments
                    .get("path")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .or_else(|| {
                        request
                            .arguments
                            .get("to")
                            .and_then(Value::as_str)
                            .map(PathBuf::from)
                    });
                match path {
                    Some(path) if self.is_path_allowed(&path) => RiskLevel::Medium,
                    Some(_) => RiskLevel::High,
                    None => RiskLevel::Medium,
                }
            }
            "run_shell" => {
                let command = request
                    .arguments
                    .get("command")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if ["rm ", "sudo ", "chmod ", "chown ", "mv "]
                    .iter()
                    .any(|needle| command.contains(needle))
                {
                    RiskLevel::Critical
                } else {
                    RiskLevel::High
                }
            }
            "applescript_run" => RiskLevel::High,
            "browser_extract" | "take_screenshot" => RiskLevel::Medium,
            _ => request.risk,
        }
    }

    fn available_tools(&self) -> &'static [&'static str] {
        &[
            "run_shell",
            "open_app",
            "list_dir",
            "create_folder",
            "read_file",
            "write_file",
            "move_path",
            "delete_path",
            "browser_open",
            "browser_extract",
            "applescript_run",
            "take_screenshot",
            "send_sms",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_path(name: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        std::env::temp_dir().join(format!("jarvis-test-{}-{}", name, millis))
    }

    #[test]
    fn delete_is_classified_as_high_risk() {
        let backend = LocalAutomationBackend::new(vec![std::env::temp_dir()]);
        let request = ToolCallRequest {
            name: "delete_path".to_string(),
            arguments: json!({ "path": "/tmp/demo" }),
            risk: RiskLevel::Low,
            requires_confirmation: false,
        };

        assert_eq!(backend.classify_risk(&request), RiskLevel::High);
    }

    #[tokio::test]
    async fn list_dir_returns_created_file() {
        let dir = unique_temp_path("list");
        fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("demo.txt");
        fs::write(&file_path, "hello").unwrap();

        let backend = LocalAutomationBackend::new(vec![std::env::temp_dir()]);
        let result = backend
            .call_tool(ToolCallRequest {
                name: "list_dir".to_string(),
                arguments: json!({ "path": dir }),
                risk: RiskLevel::Low,
                requires_confirmation: false,
            })
            .await
            .unwrap();

        assert!(result.output.unwrap_or_default().contains("demo.txt"));
    }
}
