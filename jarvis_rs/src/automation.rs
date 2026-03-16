use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
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
}

pub type LocalAutomationBackend = MacAutomationBackend;

impl MacAutomationBackend {
    pub fn new(allowed_paths: Vec<PathBuf>) -> Self {
        Self {
            allowed_paths,
            app_aliases: default_app_aliases(),
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
        #[cfg(target_os = "macos")]
        {
            self.run_applescript_lines(&vec![
                r#"tell application "Google Chrome""#.to_string(),
                "if (count of windows) = 0 then make new window".to_string(),
                r#"set resultText to execute active tab of front window javascript "document.title""#.to_string(),
                "return resultText".to_string(),
                "end tell".to_string(),
            ])
            .is_ok()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
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
        }
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
                    if normalize_name(&display_name).contains(&normalized) {
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
        let mut command = Command::new("open");
        if let Some(bundle_id) = &target.bundle_id {
            command.arg("-b").arg(bundle_id);
        } else if let Some(path) = &target.path {
            command.arg("-a").arg(path);
        } else {
            command.arg("-a").arg(&target.display_name);
        }
        command
            .status()
            .with_context(|| format!("failed to activate {}", target.display_name))?;
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    fn activate_app_target(&self, target: &AppTarget) -> Result<()> {
        let _ = target;
        bail!("app activation is only implemented on macOS");
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
}

#[derive(Debug, Deserialize)]
struct AppResolveArgs {
    app_name: String,
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
                let mut result = Self::summary(
                    "app_activate",
                    format!("Activated {}", target.display_name),
                    None,
                    serde_json::to_value(&target)?,
                );
                result.target_identity = target
                    .bundle_id
                    .clone()
                    .or_else(|| target.path.as_ref().map(|path| path.display().to_string()));
                Ok(result)
            }
            "window_snapshot" => {
                let output = self.run_applescript_lines(&vec![
                    r#"tell application "System Events""#.to_string(),
                    "set frontApp to first application process whose frontmost is true".to_string(),
                    "set appName to name of frontApp".to_string(),
                    "set windowName to \"\"".to_string(),
                    "try".to_string(),
                    "set windowName to name of front window of frontApp".to_string(),
                    "end try".to_string(),
                    "return appName & linefeed & windowName".to_string(),
                    "end tell".to_string(),
                ])?;
                let mut lines = output.lines();
                let app_name = lines.next().unwrap_or_default().to_string();
                let window_title = lines.next().unwrap_or_default().to_string();
                Ok(Self::summary(
                    "window_snapshot",
                    format!("Front window: {} {}", app_name, window_title),
                    Some(output),
                    json!({
                        "app_name": app_name,
                        "window_title": window_title,
                    }),
                ))
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
            "chrome_open_tab" | "browser_open" => {
                let args: ChromeUrlArgs = serde_json::from_value(request.arguments)?;
                let chrome = self.resolve_app_target("google chrome")?;
                self.activate_app_target(&chrome)?;
                self.run_applescript_lines(&vec![
                    r#"tell application "Google Chrome""#.to_string(),
                    "activate".to_string(),
                    "if (count of windows) = 0 then make new window".to_string(),
                    format!(
                        r#"tell front window to make new tab with properties {{URL:"{}"}}"#,
                        escape_applescript(&args.url)
                    ),
                    "end tell".to_string(),
                ])?;
                Ok(Self::summary(
                    "chrome_open_tab",
                    format!("Opened {}", args.url),
                    Some(args.url.clone()),
                    json!({
                        "browser_name": "Google Chrome",
                        "url": args.url,
                    }),
                ))
            }
            "chrome_get_dom" => {
                let observation = self.chrome_json_result(
                    r#"JSON.stringify({
                        title: document.title,
                        url: location.href,
                        bodyText: document.body ? document.body.innerText.slice(0, 8000) : "",
                        links: Array.from(document.querySelectorAll("a")).slice(0, 50).map(a => ({
                            text: (a.innerText || "").trim().slice(0, 160),
                            href: a.href || ""
                        }))
                    })"#,
                )?;
                Ok(Self::summary(
                    "chrome_get_dom",
                    "Captured Chrome DOM summary",
                    Some(observation.to_string()),
                    observation,
                ))
            }
            "chrome_click" => {
                let args: ChromeSelectorArgs = serde_json::from_value(request.arguments)?;
                let js = if let Some(selector) = args.selector {
                    format!(
                        r#"(() => {{
                            const el = document.querySelector("{}");
                            if (!el) return JSON.stringify({{ ok: false, error: "selector_not_found" }});
                            el.click();
                            return JSON.stringify({{ ok: true, mode: "selector", selector: "{}" }});
                        }})()"#,
                        escape_js_string(&selector),
                        escape_js_string(&selector)
                    )
                } else if let Some(text) = args.text {
                    format!(
                        r#"(() => {{
                            const targetText = "{}";
                            const contains = {};
                            const candidates = Array.from(document.querySelectorAll("a, button, [role='button'], span, div"));
                            const el = candidates.find(node => {{
                                const text = (node.innerText || node.textContent || "").trim();
                                return contains ? text.toLowerCase().includes(targetText.toLowerCase()) : text.toLowerCase() === targetText.toLowerCase();
                            }});
                            if (!el) return JSON.stringify({{ ok: false, error: "text_not_found", text: targetText }});
                            el.click();
                            return JSON.stringify({{ ok: true, mode: "text", text: targetText }});
                        }})()"#,
                        escape_js_string(&text),
                        if args.text_contains { "true" } else { "false" }
                    )
                } else {
                    bail!("chrome_click requires selector or text");
                };
                let observation = self.chrome_json_result(&js)?;
                Ok(Self::summary(
                    "chrome_click",
                    "Executed Chrome click",
                    Some(observation.to_string()),
                    observation,
                ))
            }
            "chrome_type" => {
                let args: ChromeTypeArgs = serde_json::from_value(request.arguments)?;
                let selector = args
                    .selector
                    .unwrap_or_else(|| "textarea, input, [contenteditable='true']".to_string());
                let js = format!(
                    r#"(() => {{
                        const el = document.querySelector("{}");
                        if (!el) return JSON.stringify({{ ok: false, error: "selector_not_found" }});
                        const value = "{}";
                        if ("value" in el) {{
                            el.focus();
                            el.value = value;
                            el.dispatchEvent(new Event("input", {{ bubbles: true }}));
                            el.dispatchEvent(new Event("change", {{ bubbles: true }}));
                        }} else {{
                            el.focus();
                            el.textContent = value;
                        }}
                        if ({}) {{
                            el.dispatchEvent(new KeyboardEvent("keydown", {{ key: "Enter", bubbles: true }}));
                        }}
                        return JSON.stringify({{ ok: true, selector: "{}", textLength: value.length }});
                    }})()"#,
                    escape_js_string(&selector),
                    escape_js_string(&args.text),
                    if args.submit { "true" } else { "false" },
                    escape_js_string(&selector)
                );
                let observation = self.chrome_json_result(&js)?;
                Ok(Self::summary(
                    "chrome_type",
                    "Executed Chrome typing",
                    Some(observation.to_string()),
                    observation,
                ))
            }
            "chrome_eval" => {
                let args: ChromeEvalArgs = serde_json::from_value(request.arguments)?;
                let observation = self.chrome_json_result(&args.script)?;
                Ok(Self::summary(
                    "chrome_eval",
                    "Executed Chrome JavaScript",
                    Some(observation.to_string()),
                    observation,
                ))
            }
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
                })
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
                self.run_applescript_lines(&vec![
                    r#"tell application id "com.apple.mail""#.to_string(),
                    "activate".to_string(),
                    format!(
                        r#"set newMessage to make new outgoing message with properties {{visible:true, subject:"{}", content:"{}"}}"#,
                        escape_applescript(&args.subject),
                        escape_applescript(&args.body)
                    ),
                    "tell newMessage".to_string(),
                    recipient_line,
                    "end tell".to_string(),
                    "end tell".to_string(),
                ])?;
                Ok(Self::summary(
                    "mail_compose",
                    "Prepared Mail draft",
                    None,
                    json!({
                        "to": args.to,
                        "subject": args.subject,
                        "body": args.body,
                    }),
                ))
            }
            "messages_compose" => {
                let args: MessagesComposeArgs = serde_json::from_value(request.arguments)?;
                let recipient = args
                    .recipient
                    .ok_or_else(|| anyhow!("messages_compose requires a recipient"))?;
                self.activate_app_target(&AppTarget {
                    requested_name: "Messages".to_string(),
                    display_name: "Messages".to_string(),
                    bundle_id: Some("com.apple.MobileSMS".to_string()),
                    path: None,
                })?;
                self.run_applescript_lines(&vec![
                    r#"tell application "System Events""#.to_string(),
                    r#"keystroke "n" using command down"#.to_string(),
                    "delay 0.4".to_string(),
                    format!(r#"keystroke "{}""#, escape_applescript(&recipient)),
                    "key code 36".to_string(),
                    "delay 0.4".to_string(),
                    format!(r#"keystroke "{}""#, escape_applescript(&args.body)),
                    "end tell".to_string(),
                ])?;
                Ok(Self::summary(
                    "messages_compose",
                    format!("Prepared message for {}", recipient),
                    None,
                    json!({
                        "recipient": recipient,
                        "body": args.body,
                    }),
                ))
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
            "chrome_eval" | "chrome_click" | "chrome_type" => RiskLevel::Medium,
            "messages_compose" | "mail_compose" => RiskLevel::High,
            _ => request.risk,
        }
    }

    fn capabilities(&self) -> AutomationCapabilities {
        AutomationCapabilities {
            tools: Self::tool_names(),
            primary_browser: "Google Chrome".to_string(),
            chrome_installed: self.chrome_installed(),
            chrome_javascript_enabled: self.chrome_javascript_enabled(),
            applescript_available: cfg!(target_os = "macos"),
            accessibility_expected: cfg!(target_os = "macos"),
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
        _ => bundle_id,
    }
    .to_string()
}

fn escape_applescript(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_js_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
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
            })
            .await
            .unwrap();

        assert!(result.output.unwrap_or_default().contains("demo.txt"));
    }
}
