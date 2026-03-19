use anyhow::{anyhow, bail, Context, Result};
use base64::Engine as _;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::browser_sidecar::{browser_target_from_value, BrowserSidecarDiagnostics, BrowserSidecarManager};
use crate::config::AppConfig;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppTarget {
    pub requested_name: String,
    pub display_name: String,
    #[serde(default)]
    pub bundle_id: Option<String>,
    #[serde(default)]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserTarget {
    pub browser_name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub tab_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiElementRef {
    pub role: String,
    pub label: String,
    #[serde(default)]
    pub identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationCapabilities {
    pub tools: Vec<String>,
    pub primary_browser: String,
    pub chrome_installed: bool,
    pub chrome_javascript_enabled: bool,
    pub applescript_available: bool,
    pub accessibility_expected: bool,
    pub browser_automation_ready: bool,
    #[serde(default)]
    pub browser_mode: Option<String>,
    #[serde(default)]
    pub setup_items: Vec<String>,
    #[serde(default)]
    pub browser_sidecar_endpoint: Option<String>,
    #[serde(default)]
    pub known_apps: Vec<AppTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub name: String,
    pub arguments: Value,
    #[serde(default = "default_risk_level")]
    pub risk: RiskLevel,
    #[serde(default)]
    pub requires_confirmation: bool,
    #[serde(default)]
    pub target_identity: Option<String>,
    #[serde(default)]
    pub expected_outcome: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub name: String,
    pub success: bool,
    pub summary: String,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub artifact_path: Option<PathBuf>,
    #[serde(default)]
    pub observation: Value,
    #[serde(default)]
    pub retryable: bool,
    #[serde(default)]
    pub target_identity: Option<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub proof_passed: bool,
    #[serde(default)]
    pub observed_outcome: Option<String>,
}

fn default_risk_level() -> RiskLevel {
    RiskLevel::Low
}

#[async_trait]
pub trait AutomationBackend: Send + Sync {
    async fn call_tool(&self, request: ToolCallRequest) -> Result<ToolCallResult>;
    fn classify_risk(&self, request: &ToolCallRequest) -> RiskLevel;
    fn capabilities(&self) -> AutomationCapabilities;
}

#[derive(Debug)]
pub struct MacAutomationBackend {
    allowed_paths: Vec<PathBuf>,
    app_aliases: HashMap<String, String>,
    primary_browser: String,
    browser_sidecar: BrowserSidecarManager,
    http_client: reqwest::Client,
    vision_api_key: Option<String>,
}

pub type LocalAutomationBackend = MacAutomationBackend;

impl MacAutomationBackend {
    pub fn new(allowed_paths: Vec<PathBuf>) -> Self {
        let mut config = AppConfig::default();
        config.allowed_paths = allowed_paths;
        Self::from_config(&config)
    }

    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            allowed_paths: config.allowed_paths.clone(),
            app_aliases: default_app_aliases(),
            primary_browser: config.primary_browser.clone(),
            browser_sidecar: BrowserSidecarManager::new(config.browser.clone()),
            http_client: reqwest::Client::new(),
            vision_api_key: config.provider.fallback_api_key.clone(),
        }
    }

    fn shell_program() -> &'static str {
        "/bin/sh"
    }

    fn shell_args(script: &str) -> [&str; 2] {
        ["-lc", script]
    }

    fn tool_names() -> Vec<String> {
        [
            "app_resolve",
            "app_activate",
            "window_snapshot",
            "ui_list_elements",
            "ui_click",
            "ui_type",
            "ui_press_key",
            "ui_select_menu",
            "ui_scroll",
            "chrome_open_tab",
            "chrome_get_dom",
            "chrome_click",
            "chrome_type",
            "chrome_eval",
            "chrome_screenshot",
            "browser_attach_or_launch",
            "browser_open",
            "browser_wait_for",
            "browser_click",
            "browser_fill",
            "browser_snapshot",
            "browser_extract_text",
            "browser_assert",
            "speak",
            "media_control",
            "screen_click",
            "mail_compose",
            "messages_compose",
            "filesystem_list",
            "filesystem_create_folder",
            "filesystem_read_file",
            "filesystem_write_file",
            "filesystem_move",
            "filesystem_delete",
            "shell_run",
            "take_screenshot",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    }

    fn known_app_targets(&self) -> Vec<AppTarget> {
        self.app_aliases
            .iter()
            .map(|(name, bundle_id)| AppTarget {
                requested_name: name.clone(),
                display_name: canonical_display_name(bundle_id),
                bundle_id: Some(bundle_id.clone()),
                path: None,
            })
            .collect()
    }

    fn chrome_installed(&self) -> bool {
        self.resolve_app_target("chrome").is_ok()
    }

    fn chrome_javascript_enabled(&self) -> bool {
        self.browser_sidecar.diagnostics().playwright_installed
    }

    fn is_path_allowed(&self, path: &Path) -> bool {
        self.allowed_paths.iter().any(|allowed| path.starts_with(allowed))
    }

    fn summary(
        name: &str,
        summary: impl Into<String>,
        output: Option<String>,
        observation: Value,
    ) -> ToolCallResult {
        ToolCallResult {
            name: name.to_string(),
            success: true,
            summary: summary.into(),
            output,
            artifact_path: None,
            observation,
            retryable: false,
            target_identity: None,
            error_code: None,
            proof_passed: true,
            observed_outcome: None,
        }
    }

    async fn screen_find_and_click(&self, label: &str) -> Result<()> {
        let api_key = self.vision_api_key.as_deref()
            .ok_or_else(|| anyhow::anyhow!("JARVIS_FALLBACK_API_KEY not set — needed for screen_click"))?;

        // 1. Take screenshot
        let path = std::env::temp_dir().join("jarvis-vision.png");
        let status = Command::new("screencapture")
            .arg("-x").arg("-t").arg("png").arg(&path)
            .status().context("screencapture failed")?;
        if !status.success() {
            bail!("screencapture exited with error");
        }

        // 2. Base64 encode
        let bytes = fs::read(&path).context("failed to read screenshot")?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

        // 3. Get logical screen size via AppleScript
        let dims = self.run_applescript_lines(&[
            r#"tell application "Finder""#.to_string(),
            r#"set b to bounds of window of desktop"#.to_string(),
            r#"return ((item 3 of b) as string) & "," & ((item 4 of b) as string)"#.to_string(),
            "end tell".to_string(),
        ]).unwrap_or_else(|_| "1920,1080".to_string());
        let mut parts = dims.trim().splitn(2, ',');
        let sw: f64 = parts.next().and_then(|s| s.trim().parse().ok()).unwrap_or(1920.0);
        let sh: f64 = parts.next().and_then(|s| s.trim().parse().ok()).unwrap_or(1080.0);

        // 4. Ask GPT-4o-mini vision for element coordinates (as 0-1 fractions)
        let resp: serde_json::Value = self.http_client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(api_key)
            .json(&serde_json::json!({
                "model": "gpt-4o-mini",
                "max_tokens": 60,
                "messages": [{
                    "role": "user",
                    "content": [
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:image/png;base64,{}", b64),
                                "detail": "high"
                            }
                        },
                        {
                            "type": "text",
                            "text": format!(
                                "This is a macOS screenshot. Find the UI element matching '{}'. Return ONLY valid JSON with no markdown: {{\"x\": <0.0-1.0 fraction from left edge>, \"y\": <0.0-1.0 fraction from top edge>}} for the exact center of that element. If multiple matches exist, pick the most prominent one.",
                                label
                            )
                        }
                    ]
                }]
            }))
            .send().await?.error_for_status()?.json().await?;

        let raw = resp["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string();
        let json_str = raw
            .strip_prefix("```json").or_else(|| raw.strip_prefix("```"))
            .and_then(|s| s.rsplit_once("```").map(|(inner, _)| inner.trim()))
            .unwrap_or(&raw);

        let coords: serde_json::Value = serde_json::from_str(json_str)
            .with_context(|| format!("vision model returned: {}", raw))?;

        let fx = coords["x"].as_f64().context("no x coordinate in vision response")?;
        let fy = coords["y"].as_f64().context("no y coordinate in vision response")?;
        let cx = (fx * sw) as i64;
        let cy = (fy * sh) as i64;

        // 5. Click at coordinates
        self.run_applescript_lines(&[
            r#"tell application "System Events""#.to_string(),
            format!("click at {{{}, {}}}", cx, cy),
            "end tell".to_string(),
        ])?;

        Ok(())
    }

    fn resolve_app_target(&self, app_name: &str) -> Result<AppTarget> {
        let requested = app_name.trim();
        if requested.is_empty() {
            bail!("app name cannot be empty");
        }

        let normalized = normalize_name(requested);
        if let Some(bundle_id) = self.app_aliases.get(&normalized) {
            return Ok(AppTarget {
                requested_name: requested.to_string(),
                display_name: canonical_display_name(bundle_id),
                bundle_id: Some(bundle_id.clone()),
                path: None,
            });
        }

        for directory in app_search_paths() {
            if !directory.exists() {
                continue;
            }
            if let Ok(entries) = fs::read_dir(&directory) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let is_app = path
                        .extension()
                        .and_then(|value| value.to_str())
                        .map(|value| value.eq_ignore_ascii_case("app"))
                        .unwrap_or(false);
                    if !is_app {
                        continue;
                    }
                    let display_name = path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_string();
                    let normalized_display = normalize_name(&display_name);
                    if normalized_display.contains(&normalized) || normalized.contains(&normalized_display) {
                        return Ok(AppTarget {
                            requested_name: requested.to_string(),
                            display_name,
                            bundle_id: None,
                            path: Some(path),
                        });
                    }
                }
            }
        }

        bail!("could not resolve application '{}'", requested);
    }

    #[cfg(target_os = "macos")]
    fn activate_app_target(&self, target: &AppTarget) -> Result<()> {
        let already_running = self.app_is_running(&target.display_name);

        if already_running {
            // Focus the existing instance via AppleScript — never re-launch.
            if let Some(bundle_id) = &target.bundle_id {
                let _ = self.run_applescript_lines(&[
                    format!(r#"tell application id "{}" to activate"#, escape_applescript(bundle_id)),
                ]);
            } else {
                // Try exact name first, then process name partial match.
                let exact = self.run_applescript_lines(&[
                    format!(r#"tell application "{}" to activate"#, escape_applescript(&target.display_name)),
                ]);
                if exact.is_err() {
                    let _ = self.run_applescript_lines(&[
                        r#"tell application "System Events""#.to_string(),
                        format!(
                            r#"set p to first process whose name contains "{}""#,
                            escape_applescript(&target.display_name.split_whitespace().next().unwrap_or(&target.display_name))
                        ),
                        "set frontmost of p to true".to_string(),
                        "end tell".to_string(),
                    ]);
                }
            }
            // Give it a moment to come to front; don't re-launch regardless.
            thread::sleep(Duration::from_millis(500));
            return Ok(());
        }

        // App not running — launch it by name so macOS picks the installed copy.
        let mut command = Command::new("open");
        if let Some(bundle_id) = &target.bundle_id {
            command.arg("-b").arg(bundle_id);
        } else {
            command.arg("-a").arg(&target.display_name);
        }
        let status = command
            .status()
            .with_context(|| format!("failed to launch {}", target.display_name))?;
        if !status.success() {
            bail!("macOS failed to launch {}", target.display_name);
        }

        // Wait for it to appear as frontmost (relaxed: contains match).
        let normalized = target.display_name.to_ascii_lowercase();
        for _ in 0..16 {
            thread::sleep(Duration::from_millis(500));
            if let Ok(frontmost) = self.frontmost_app_name() {
                if frontmost.to_ascii_lowercase().contains(&normalized)
                    || normalized.contains(&frontmost.to_ascii_lowercase())
                {
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    fn app_is_running(&self, display_name: &str) -> bool {
        let first_word = display_name
            .split_whitespace()
            .next()
            .unwrap_or(display_name);
        Command::new("pgrep")
            .arg("-ix")
            .arg(display_name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
            || Command::new("pgrep")
                .arg("-i")
                .arg(first_word)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
    }

    #[cfg(not(target_os = "macos"))]
    fn activate_app_target(&self, target: &AppTarget) -> Result<()> {
        let _ = target;
        bail!("app activation is only implemented on macOS");
    }

    #[cfg(target_os = "macos")]
    fn frontmost_app_name(&self) -> Result<String> {
        let output = Command::new(Self::shell_program())
            .args(Self::shell_args("lsappinfo info -only name `lsappinfo front`"))
            .output()
            .context("failed to run lsappinfo")?;
        if !output.status.success() {
            bail!(
                "failed to query frontmost app: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let name = stdout
            .split('"')
            .nth(3)
            .map(str::to_string)
            .or_else(|| stdout.split('=').nth(1).map(|value| value.trim_matches('"').trim().to_string()))
            .unwrap_or_else(|| stdout.trim().to_string());
        if name.is_empty() {
            bail!("frontmost app query returned an empty name");
        }
        Ok(name)
    }

    #[cfg(not(target_os = "macos"))]
    fn frontmost_app_name(&self) -> Result<String> {
        bail!("frontmost app detection is only available on macOS");
    }

    #[cfg(target_os = "macos")]
    fn run_applescript_lines(&self, lines: &[String]) -> Result<String> {
        let mut command = Command::new("osascript");
        for line in lines {
            command.arg("-e").arg(line);
        }
        let output = command.output().context("failed to execute osascript")?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !output.status.success() {
            bail!(
                "AppleScript failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(stdout)
    }

    #[cfg(not(target_os = "macos"))]
    fn run_applescript_lines(&self, _lines: &[String]) -> Result<String> {
        bail!("AppleScript is only available on macOS");
    }

    fn run_chrome_javascript(&self, script: &str) -> Result<String> {
        let lines = vec![
            r#"tell application "Google Chrome""#.to_string(),
            "activate".to_string(),
            r#"if (count of windows) = 0 then error "Google Chrome has no open windows""#.to_string(),
            format!(
                r#"set resultText to execute active tab of front window javascript "{}""#,
                escape_applescript(script)
            ),
            "return resultText".to_string(),
            "end tell".to_string(),
        ];
        self.run_applescript_lines(&lines).map_err(|error| {
            let message = error.to_string();
            if message.contains("Executing JavaScript through AppleScript is turned off") {
                anyhow!(
                    "Chrome JavaScript automation is disabled. In Chrome, open View > Developer > Allow JavaScript from Apple Events, then retry."
                )
            } else {
                error
            }
        })
    }

    fn chrome_json_result(&self, script: &str) -> Result<Value> {
        let output = self.run_chrome_javascript(script)?;
        if output.trim().is_empty() {
            return Ok(json!({}));
        }
        serde_json::from_str(&output).or_else(|_| Ok(json!({ "raw": output })))
    }

    fn browser_diagnostics(&self) -> BrowserSidecarDiagnostics {
        self.browser_sidecar.diagnostics()
    }

    fn accessibility_available(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.run_applescript_lines(&vec![
                r#"tell application "System Events""#.to_string(),
                "return count of application processes".to_string(),
                "end tell".to_string(),
            ])
            .is_ok()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    async fn call_browser_tool(
        &self,
        tool_name: &str,
        route: &str,
        request_arguments: Value,
        summary_template: &str,
    ) -> Result<ToolCallResult> {
        let observation = self.browser_sidecar.call(route, request_arguments).await?;
        let mut result = ToolCallResult {
            name: tool_name.to_string(),
            success: observation.get("ok").and_then(Value::as_bool).unwrap_or(true),
            summary: summary_template.to_string(),
            output: Some(observation.to_string()),
            artifact_path: None,
            observation: browser_target_from_value(&observation),
            retryable: true,
            target_identity: observation
                .get("url")
                .and_then(Value::as_str)
                .map(str::to_string),
            error_code: None,
            proof_passed: observation
                .get("matched")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            observed_outcome: observation
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| observation.get("url").and_then(Value::as_str).map(str::to_string)),
        };

        if let Some(extracted) = observation.get("extractedText").and_then(Value::as_str) {
            result.output = Some(extracted.to_string());
            result.observation = json!({
                "browser_name": observation.get("browserName").and_then(Value::as_str).unwrap_or("Google Chrome"),
                "url": observation.get("url").and_then(Value::as_str),
                "tab_hint": observation.get("title").and_then(Value::as_str),
                "extracted_text": extracted,
            });
        } else if let Some(text) = observation.get("visibleText").and_then(Value::as_str) {
            result.output = Some(text.to_string());
        }

        if let Some(matched) = observation.get("matched").and_then(Value::as_bool) {
            result.summary = if matched {
                format!("{} (verified)", summary_template)
            } else {
                format!("{} (not yet verified)", summary_template)
            };
        }

        Ok(result)
    }
}

#[derive(Debug, Deserialize)]
struct AppResolveArgs {
    app_name: String,
}

#[derive(Debug, Deserialize)]
struct ScreenClickArgs {
    label: String,
}

#[derive(Debug, Deserialize)]
struct AppActivateArgs {
    app_name: Option<String>,
    bundle_id: Option<String>,
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
struct ShellArgs {
    command: String,
}

#[derive(Debug, Deserialize)]
struct ChromeUrlArgs {
    url: String,
}

#[derive(Debug, Deserialize)]
struct ChromeSelectorArgs {
    selector: Option<String>,
    text: Option<String>,
    #[serde(default)]
    pub text_contains: bool,
}

#[derive(Debug, Deserialize)]
struct ChromeTypeArgs {
    selector: Option<String>,
    text: String,
    #[serde(default)]
    submit: bool,
}

#[derive(Debug, Deserialize)]
struct ChromeEvalArgs {
    script: String,
}

#[derive(Debug, Deserialize)]
struct UiTypeArgs {
    text: String,
}

#[derive(Debug, Deserialize)]
struct UiPressKeyArgs {
    key: String,
    #[serde(default)]
    modifiers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UiScrollArgs {
    amount: i32,
}

#[derive(Debug, Deserialize)]
struct UiClickArgs {
    label: String,
}

#[derive(Debug, Deserialize)]
struct UiSelectMenuArgs {
    menu: String,
    item: String,
}

#[derive(Debug, Deserialize)]
struct ScreenshotArgs {
    path: String,
}

#[derive(Debug, Deserialize)]
struct MailComposeArgs {
    to: Option<String>,
    subject: String,
    body: String,
}

#[derive(Debug, Deserialize)]
struct MessagesComposeArgs {
    recipient: Option<String>,
    body: String,
}

#[async_trait]
impl AutomationBackend for MacAutomationBackend {
    async fn call_tool(&self, request: ToolCallRequest) -> Result<ToolCallResult> {
        match request.name.as_str() {
            "app_resolve" => {
                let args: AppResolveArgs = serde_json::from_value(request.arguments)?;
                let target = self.resolve_app_target(&args.app_name)?;
                let observation = serde_json::to_value(&target)?;
                Ok(Self::summary(
                    "app_resolve",
                    format!("Resolved {}", target.display_name),
                    Some(target.display_name.clone()),
                    observation,
                ))
            }
            "app_activate" => {
                let args: AppActivateArgs = serde_json::from_value(request.arguments)?;
                let target = if let Some(bundle_id) = args.bundle_id {
                    AppTarget {
                        requested_name: args.app_name.unwrap_or_else(|| bundle_id.clone()),
                        display_name: canonical_display_name(&bundle_id),
                        bundle_id: Some(bundle_id),
                        path: None,
                    }
                } else {
                    let app_name = args
                        .app_name
                        .ok_or_else(|| anyhow!("app_activate requires app_name or bundle_id"))?;
                    self.resolve_app_target(&app_name)?
                };
                self.activate_app_target(&target)?;
                let frontmost = self
                    .frontmost_app_name()
                    .unwrap_or_else(|_| target.display_name.clone());
                let mut result = Self::summary(
                    "app_activate",
                    format!("Activated {}", target.display_name),
                    None,
                    json!({
                        "requested_name": target.requested_name,
                        "display_name": target.display_name,
                        "bundle_id": target.bundle_id,
                        "path": target.path,
                        "frontmost_app": frontmost,
                        "matched": frontmost == target.display_name,
                    }),
                );
                result.target_identity = target
                    .bundle_id
                    .clone()
                    .or_else(|| target.path.as_ref().map(|path| path.display().to_string()));
                Ok(result)
            }
            "window_snapshot" => {
                let app_name = self.frontmost_app_name()?;
                let window_title = String::new();
                let mut result = Self::summary(
                    "window_snapshot",
                    format!("Front window: {} {}", app_name, window_title),
                    Some(app_name.clone()),
                    json!({
                        "app_name": app_name,
                        "window_title": window_title,
                    }),
                );
                result.observed_outcome = Some(app_name.clone());
                Ok(result)
            }
            "ui_list_elements" => Ok(Self::summary(
                "ui_list_elements",
                "UI element enumeration is not yet available for arbitrary apps; use window_snapshot or app-specific tools first",
                None,
                json!({"available": false}),
            )),
            "ui_click" => {
                let args: UiClickArgs = serde_json::from_value(request.arguments)?;
                let script = vec![
                    r#"tell application "System Events""#.to_string(),
                    "tell first application process whose frontmost is true".to_string(),
                    format!(r#"click button "{}" of front window"#, escape_applescript(&args.label)),
                    "end tell".to_string(),
                    "end tell".to_string(),
                ];
                self.run_applescript_lines(&script)?;
                Ok(Self::summary(
                    "ui_click",
                    format!("Clicked {}", args.label),
                    None,
                    json!({"label": args.label}),
                ))
            }
            "ui_type" => {
                let args: UiTypeArgs = serde_json::from_value(request.arguments)?;
                self.run_applescript_lines(&vec![
                    r#"tell application "System Events""#.to_string(),
                    format!(r#"keystroke "{}""#, escape_applescript(&args.text)),
                    "end tell".to_string(),
                ])?;
                Ok(Self::summary(
                    "ui_type",
                    "Typed text".to_string(),
                    None,
                    json!({"text_len": args.text.chars().count()}),
                ))
            }
            "ui_press_key" => {
                let args: UiPressKeyArgs = serde_json::from_value(request.arguments)?;
                let key_command = applescript_key_command(&args.key, &args.modifiers)?;
                self.run_applescript_lines(&vec![
                    r#"tell application "System Events""#.to_string(),
                    key_command,
                    "end tell".to_string(),
                ])?;
                Ok(Self::summary(
                    "ui_press_key",
                    format!("Pressed {}", args.key),
                    None,
                    json!({"key": args.key, "modifiers": args.modifiers}),
                ))
            }
            "ui_select_menu" => {
                let args: UiSelectMenuArgs = serde_json::from_value(request.arguments)?;
                self.run_applescript_lines(&vec![
                    r#"tell application "System Events""#.to_string(),
                    "tell first application process whose frontmost is true".to_string(),
                    format!(
                        r#"click menu item "{}" of menu "{}" of menu bar 1"#,
                        escape_applescript(&args.item),
                        escape_applescript(&args.menu)
                    ),
                    "end tell".to_string(),
                    "end tell".to_string(),
                ])?;
                Ok(Self::summary(
                    "ui_select_menu",
                    format!("Selected {} > {}", args.menu, args.item),
                    None,
                    json!({"menu": args.menu, "item": args.item}),
                ))
            }
            "ui_scroll" => {
                let args: UiScrollArgs = serde_json::from_value(request.arguments)?;
                self.run_applescript_lines(&vec![
                    r#"tell application "System Events""#.to_string(),
                    format!("key code 125 {}", if args.amount < 0 { "" } else { "" }),
                    "end tell".to_string(),
                ])?;
                Ok(Self::summary(
                    "ui_scroll",
                    format!("Requested scroll {}", args.amount),
                    None,
                    json!({"amount": args.amount}),
                ))
            }
            "browser_attach_or_launch" => self.call_browser_tool(
                "browser_attach_or_launch",
                "/browser/attach_or_launch",
                request.arguments,
                "Attached to or launched a browser session",
            ).await,
            "chrome_open_tab" | "browser_open" => {
                let args: ChromeUrlArgs = serde_json::from_value(request.arguments)?;
                self.call_browser_tool(
                    "browser_open",
                    "/browser/open",
                    json!({ "url": args.url }),
                    "Opened browser page",
                ).await
            }
            "chrome_get_dom" | "browser_snapshot" => self.call_browser_tool(
                "browser_snapshot",
                "/browser/snapshot",
                request.arguments,
                "Captured browser snapshot",
            ).await,
            "browser_wait_for" => self.call_browser_tool(
                "browser_wait_for",
                "/browser/wait_for",
                request.arguments,
                "Waited for browser state",
            ).await,
            "chrome_click" | "browser_click" => {
                let args: ChromeSelectorArgs = serde_json::from_value(request.arguments)?;
                self.call_browser_tool(
                    "browser_click",
                    "/browser/click",
                    json!({
                        "selector": args.selector,
                        "text": args.text,
                        "exact": !args.text_contains,
                    }),
                    "Executed browser click",
                ).await
            }
            "chrome_type" | "browser_fill" => {
                let args: ChromeTypeArgs = serde_json::from_value(request.arguments)?;
                self.call_browser_tool(
                    "browser_fill",
                    "/browser/fill",
                    json!({
                        "selector": args.selector,
                        "text": args.text,
                        "submit": args.submit,
                    }),
                    "Filled browser field",
                ).await
            }
            "chrome_eval" => {
                let args: ChromeEvalArgs = serde_json::from_value(request.arguments)?;
                let observation = self.chrome_json_result(&args.script)?;
                let mut result = Self::summary(
                    "chrome_eval",
                    "Executed Chrome JavaScript",
                    Some(observation.to_string()),
                    observation,
                );
                result.proof_passed = true;
                Ok(result)
            }
            "browser_extract_text" => self.call_browser_tool(
                "browser_extract_text",
                "/browser/extract_text",
                request.arguments,
                "Extracted browser text",
            ).await,
            "browser_assert" => self.call_browser_tool(
                "browser_assert",
                "/browser/assert",
                request.arguments,
                "Asserted browser state",
            ).await,
            "chrome_screenshot" | "take_screenshot" => {
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
                    name: request.name,
                    success: true,
                    summary: format!("Captured screenshot to {}", args.path),
                    output: None,
                    artifact_path: Some(PathBuf::from(&args.path)),
                    observation: json!({ "path": args.path }),
                    retryable: true,
                    target_identity: None,
                    error_code: None,
                    proof_passed: true,
                    observed_outcome: Some(args.path),
                })
            }
            "speak" => {
                let message = request.arguments["message"].as_str().unwrap_or("").to_string();
                Ok(Self::summary(
                    "speak",
                    message.clone(),
                    Some(message),
                    json!({"spoken": true}),
                ))
            }
            "media_control" => {
                // Native AppleScript control for Spotify / Music / system media
                let action = request.arguments["action"].as_str().unwrap_or("play").to_string();
                let app = request.arguments["app"].as_str().unwrap_or("Spotify").to_string();
                let script_action = match action.to_lowercase().as_str() {
                    "pause" | "stop" => "pause",
                    "next" | "next_track" => "next track",
                    "previous" | "prev" | "previous_track" => "previous track",
                    _ => "play", // play / resume / toggle all map to "play"
                };
                let result = self.run_applescript_lines(&[
                    format!("tell application \"{}\"", app),
                    script_action.to_string(),
                    "end tell".to_string(),
                ]);
                match result {
                    Ok(_) => Ok(Self::summary(
                        "media_control",
                        format!("{} {}", script_action, app),
                        Some(format!("Sent '{}' command to {}", script_action, app)),
                        json!({"action": script_action, "app": app}),
                    )),
                    Err(e) => bail!("media_control failed: {}", e),
                }
            }
            "screen_click" => {
                let args: ScreenClickArgs = serde_json::from_value(request.arguments)?;
                self.screen_find_and_click(&args.label).await?;
                Ok(Self::summary(
                    "screen_click",
                    format!("Clicked '{}'", args.label),
                    Some(format!("Found and clicked '{}'", args.label)),
                    json!({"label": args.label}),
                ))
            }
            "mail_compose" => {
                let args: MailComposeArgs = serde_json::from_value(request.arguments)?;
                let recipient_line = args
                    .to
                    .as_deref()
                    .map(|to| {
                        format!(
                            "make new to recipient at end of to recipients with properties {{address:\"{}\"}}",
                            escape_applescript(to)
                        )
                    })
                    .unwrap_or_default();
                let output = self.run_applescript_lines(&vec![
                    r#"tell application id "com.apple.mail""#.to_string(),
                    "activate".to_string(),
                    format!(
                        r#"set newMessage to make new outgoing message with properties {{visible:true, subject:"{}", content:"{}"}}"#,
                        escape_applescript(&args.subject),
                        escape_applescript(&args.body)
                    ),
                    "tell newMessage".to_string(),
                    recipient_line,
                    "delay 0.2".to_string(),
                    "set verifiedSubject to subject of newMessage".to_string(),
                    "set verifiedContent to content of newMessage".to_string(),
                    "set verifiedRecipients to address of every to recipient of newMessage".to_string(),
                    r#"return verifiedSubject & linefeed & (verifiedRecipients as string) & linefeed & verifiedContent"#.to_string(),
                    "end tell".to_string(),
                    "end tell".to_string(),
                ])?;
                let mut lines = output.lines();
                let verified_subject = lines.next().unwrap_or_default().to_string();
                let verified_recipients = lines.next().unwrap_or_default().to_string();
                let verified_content = lines.collect::<Vec<_>>().join("\n");
                let proof_passed = verified_subject == args.subject
                    && verified_content.contains(&args.body)
                    && args
                        .to
                        .as_deref()
                        .map(|to| verified_recipients.contains(to))
                        .unwrap_or(true);
                let mut result = Self::summary(
                    "mail_compose",
                    if proof_passed {
                        "Prepared Mail draft (verified)"
                    } else {
                        "Prepared Mail draft (verification incomplete)"
                    },
                    Some(verified_content.clone()),
                    json!({
                        "to": args.to,
                        "subject": args.subject,
                        "body": args.body,
                        "verified_recipients": verified_recipients,
                    }),
                );
                result.target_identity = Some("com.apple.mail".to_string());
                result.proof_passed = proof_passed;
                result.observed_outcome = Some(verified_subject);
                Ok(result)
            }
            "messages_compose" => {
                let args: MessagesComposeArgs = serde_json::from_value(request.arguments)?;
                let recipient = args
                    .recipient
                    .ok_or_else(|| anyhow!("messages_compose requires a recipient"))?;

                // Activate Messages first, then open the sms: URL to pre-fill recipient + body
                self.run_applescript_lines(&[
                    r#"tell application "Messages" to activate"#.to_string(),
                ])?;
                thread::sleep(Duration::from_millis(400));

                let url = format!(
                    "sms:{}&body={}",
                    percent_encode_for_url(&recipient),
                    percent_encode_for_url(&args.body)
                );
                // Use do shell script to open the URL — more reliable than AppleScript open location
                self.run_applescript_lines(&[
                    format!(r#"do shell script "open '{}'"#, url.replace('\'', "%27")),
                ])?;
                thread::sleep(Duration::from_millis(800));

                let frontmost = self.frontmost_app_name().unwrap_or_default();
                let window_count = self.run_applescript_lines(&[
                    r#"tell application id "com.apple.MobileSMS""#.to_string(),
                    "return (count of windows) as string".to_string(),
                    "end tell".to_string(),
                ]).ok().and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(0);

                let proof_passed = frontmost == "Messages" && window_count > 0;
                let mut result = Self::summary(
                    "messages_compose",
                    if proof_passed {
                        format!("Opened Messages compose for {} (verified)", recipient)
                    } else {
                        format!("Opened Messages for {} (verification incomplete)", recipient)
                    },
                    Some(args.body.clone()),
                    json!({
                        "recipient": recipient,
                        "body": args.body,
                        "window_count": window_count,
                        "frontmost_app": frontmost,
                    }),
                );
                result.target_identity = Some("com.apple.MobileSMS".to_string());
                result.proof_passed = proof_passed;
                result.observed_outcome = Some(format!("Messages open with {} windows", window_count));
                Ok(result)
            }
            "filesystem_list" => {
                let args: PathArgs = serde_json::from_value(request.arguments)?;
                let entries = fs::read_dir(&args.path)
                    .with_context(|| format!("failed to list {}", args.path))?
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.file_name().to_string_lossy().to_string())
                    .collect::<Vec<_>>();
                Ok(Self::summary(
                    "filesystem_list",
                    format!("Listed {}", args.path),
                    Some(entries.join("\n")),
                    json!({ "path": args.path, "entries": entries }),
                ))
            }
            "filesystem_create_folder" => {
                let args: PathArgs = serde_json::from_value(request.arguments)?;
                fs::create_dir_all(&args.path)
                    .with_context(|| format!("failed to create {}", args.path))?;
                Ok(Self::summary(
                    "filesystem_create_folder",
                    format!("Created folder {}", args.path),
                    None,
                    json!({ "path": args.path }),
                ))
            }
            "filesystem_read_file" => {
                let args: PathArgs = serde_json::from_value(request.arguments)?;
                let contents =
                    fs::read_to_string(&args.path).with_context(|| format!("failed to read {}", args.path))?;
                Ok(Self::summary(
                    "filesystem_read_file",
                    format!("Read {}", args.path),
                    Some(contents.clone()),
                    json!({ "path": args.path, "size": contents.len() }),
                ))
            }
            "filesystem_write_file" => {
                let args: WriteFileArgs = serde_json::from_value(request.arguments)?;
                fs::write(&args.path, &args.contents)
                    .with_context(|| format!("failed to write {}", args.path))?;
                Ok(Self::summary(
                    "filesystem_write_file",
                    format!("Wrote {}", args.path),
                    None,
                    json!({ "path": args.path, "size": args.contents.len() }),
                ))
            }
            "filesystem_move" => {
                let args: MovePathArgs = serde_json::from_value(request.arguments)?;
                fs::rename(&args.from, &args.to)
                    .with_context(|| format!("failed to move {} to {}", args.from, args.to))?;
                Ok(Self::summary(
                    "filesystem_move",
                    format!("Moved {} to {}", args.from, args.to),
                    None,
                    json!({ "from": args.from, "to": args.to }),
                ))
            }
            "filesystem_delete" => {
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
                    "filesystem_delete",
                    format!("Deleted {}", args.path),
                    None,
                    json!({ "path": args.path }),
                ))
            }
            "shell_run" => {
                let args: ShellArgs = serde_json::from_value(request.arguments)?;
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
                Ok(Self::summary(
                    "shell_run",
                    "Shell command completed",
                    Some(combined.clone()),
                    json!({ "command": args.command, "output": combined }),
                ))
            }
            other => Err(anyhow!("unsupported tool: {}", other)),
        }
    }

    fn classify_risk(&self, request: &ToolCallRequest) -> RiskLevel {
        match request.name.as_str() {
            "filesystem_delete" => RiskLevel::High,
            "filesystem_write_file" | "filesystem_move" => {
                let path = request
                    .arguments
                    .get("path")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .or_else(|| request.arguments.get("to").and_then(Value::as_str).map(PathBuf::from));
                match path {
                    Some(path) if self.is_path_allowed(&path) => RiskLevel::Medium,
                    Some(_) => RiskLevel::High,
                    None => RiskLevel::Medium,
                }
            }
            "shell_run" => {
                let command = request
                    .arguments
                    .get("command")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if ["rm ", "sudo ", "chmod ", "chown ", "mv ", "kill "]
                    .iter()
                    .any(|needle| command.contains(needle))
                {
                    RiskLevel::Critical
                } else {
                    RiskLevel::High
                }
            }
            "ui_click" | "ui_type" | "ui_press_key" | "ui_select_menu" | "ui_scroll" => RiskLevel::High,
            "chrome_eval" | "chrome_click" | "chrome_type" | "browser_click" | "browser_fill" => RiskLevel::Medium,
            "media_control" => RiskLevel::Low,
            "screen_click" => RiskLevel::Medium,
            "messages_compose" | "mail_compose" => RiskLevel::Medium,
            _ => request.risk,
        }
    }

    fn capabilities(&self) -> AutomationCapabilities {
        let browser = self.browser_diagnostics();
        let accessibility_expected = cfg!(target_os = "macos");
        let accessibility_available = self.accessibility_available();
        let mut setup_items = browser.setup_items;
        if accessibility_expected && !accessibility_available {
            setup_items.push(
                "Accessibility permission is blocked for automation; native UI inspection will stay limited until macOS grants assistive access.".to_string(),
            );
        }
        AutomationCapabilities {
            tools: Self::tool_names(),
            primary_browser: self.primary_browser.clone(),
            chrome_installed: self.chrome_installed(),
            chrome_javascript_enabled: self.chrome_javascript_enabled(),
            applescript_available: cfg!(target_os = "macos"),
            accessibility_expected,
            browser_automation_ready: browser.chrome_ready && browser.playwright_installed,
            browser_mode: browser.browser_mode,
            setup_items,
            browser_sidecar_endpoint: Some(browser.endpoint),
            known_apps: self.known_app_targets(),
        }
    }
}

fn app_search_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("/Applications"), PathBuf::from("/System/Applications")];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        paths.push(home.join("Applications"));
    }
    paths
}

fn default_app_aliases() -> HashMap<String, String> {
    HashMap::from([
        ("chrome".to_string(), "com.google.Chrome".to_string()),
        ("google chrome".to_string(), "com.google.Chrome".to_string()),
        ("mail".to_string(), "com.apple.mail".to_string()),
        ("apple mail".to_string(), "com.apple.mail".to_string()),
        ("messages".to_string(), "com.apple.MobileSMS".to_string()),
        ("imessage".to_string(), "com.apple.MobileSMS".to_string()),
        ("safari".to_string(), "com.apple.Safari".to_string()),
        ("finder".to_string(), "com.apple.finder".to_string()),
        ("notes".to_string(), "com.apple.Notes".to_string()),
        ("zoom".to_string(), "us.zoom.xos".to_string()),
        ("zoom app".to_string(), "us.zoom.xos".to_string()),
        ("zoom application".to_string(), "us.zoom.xos".to_string()),
        ("zoom us".to_string(), "us.zoom.xos".to_string()),
        ("spotify".to_string(), "com.spotify.client".to_string()),
        ("slack".to_string(), "com.tinyspeck.slackmacgap".to_string()),
        ("discord".to_string(), "com.hnc.Discord".to_string()),
        ("vscode".to_string(), "com.microsoft.VSCode".to_string()),
        ("vs code".to_string(), "com.microsoft.VSCode".to_string()),
        ("visual studio code".to_string(), "com.microsoft.VSCode".to_string()),
        ("terminal".to_string(), "com.apple.Terminal".to_string()),
        ("xcode".to_string(), "com.apple.dt.Xcode".to_string()),
    ])
}

fn normalize_name(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn canonical_display_name(bundle_id: &str) -> String {
    match bundle_id {
        "com.google.Chrome" => "Google Chrome",
        "com.apple.mail" => "Mail",
        "com.apple.MobileSMS" => "Messages",
        "com.apple.Safari" => "Safari",
        "com.apple.finder" => "Finder",
        "com.apple.Notes" => "Notes",
        "com.apple.Terminal" => "Terminal",
        "com.apple.dt.Xcode" => "Xcode",
        "us.zoom.xos" => "zoom.us",
        "com.spotify.client" => "Spotify",
        "com.tinyspeck.slackmacgap" => "Slack",
        "com.hnc.Discord" => "Discord",
        "com.microsoft.VSCode" => "Visual Studio Code",
        _ => bundle_id,
    }
    .to_string()
}

fn escape_applescript(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn percent_encode_for_url(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "%20".to_string(),
            other => format!("%{:02X}", other),
        })
        .collect::<String>()
}

fn applescript_key_command(key: &str, modifiers: &[String]) -> Result<String> {
    let normalized_key = key.to_ascii_lowercase();
    let using = if modifiers.is_empty() {
        String::new()
    } else {
        let mapped = modifiers
            .iter()
            .map(|modifier| match modifier.to_ascii_lowercase().as_str() {
                "command" | "cmd" => "command down".to_string(),
                "shift" => "shift down".to_string(),
                "option" | "alt" => "option down".to_string(),
                "control" | "ctrl" => "control down".to_string(),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(" using {{{}}}", mapped)
    };

    let result = match normalized_key.as_str() {
        "enter" | "return" => format!("key code 36{}", using),
        "tab" => format!("key code 48{}", using),
        "space" => format!("key code 49{}", using),
        "escape" | "esc" => format!("key code 53{}", using),
        "up" => format!("key code 126{}", using),
        "down" => format!("key code 125{}", using),
        "left" => format!("key code 123{}", using),
        "right" => format!("key code 124{}", using),
        value if value.len() == 1 => format!(r#"keystroke "{}"{}"#, escape_applescript(value), using),
        _ => bail!("unsupported key '{}'", key),
    };

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let backend = MacAutomationBackend::new(vec![std::env::temp_dir()]);
        let request = ToolCallRequest {
            name: "filesystem_delete".to_string(),
            arguments: json!({ "path": "/tmp/demo" }),
            risk: RiskLevel::Low,
            requires_confirmation: false,
            target_identity: None,
            expected_outcome: None,
        };

        assert_eq!(backend.classify_risk(&request), RiskLevel::High);
    }

    #[test]
    fn resolves_known_app_without_finder_fallback() {
        let backend = MacAutomationBackend::new(vec![std::env::temp_dir()]);
        let target = backend.resolve_app_target("chrome").unwrap();

        assert_eq!(target.bundle_id.as_deref(), Some("com.google.Chrome"));
        assert_ne!(target.display_name, "Finder");
    }

    #[test]
    fn unknown_app_returns_error() {
        let backend = MacAutomationBackend::new(vec![std::env::temp_dir()]);
        let result = backend.resolve_app_target("totally imaginary app name");

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn filesystem_list_returns_created_file() {
        let dir = unique_temp_path("list");
        fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("demo.txt");
        fs::write(&file_path, "hello").unwrap();

        let backend = MacAutomationBackend::new(vec![std::env::temp_dir()]);
        let result = backend
            .call_tool(ToolCallRequest {
                name: "filesystem_list".to_string(),
                arguments: json!({ "path": dir }),
                risk: RiskLevel::Low,
                requires_confirmation: false,
                target_identity: None,
                expected_outcome: None,
            })
            .await
            .unwrap();

        assert!(result.output.unwrap_or_default().contains("demo.txt"));
    }
}
