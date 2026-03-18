use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};

use jarvis_rs::automation::{
    AutomationBackend, LocalAutomationBackend, ToolCallRequest, ToolCallResult,
};
use jarvis_rs::browser_sidecar::BrowserSidecarManager;
use jarvis_rs::config::AppConfig;
use jarvis_rs::logging;

const GMAIL_INSPECTION_SCRIPT: &str = r#"
const expected = arg || {};
const normalize = value => String(value || '').replace(/\s+/g, ' ').trim();
const lower = value => normalize(value).toLowerCase();
const pageText = normalize(document.body ? document.body.innerText || '' : '');
const subjectInput = document.querySelector('input[name="subjectbox"]');
const subjectValue = normalize(subjectInput && 'value' in subjectInput ? subjectInput.value : '');
const bodyCandidates = Array.from(document.querySelectorAll('div[role="textbox"], div[aria-label*="Message Body"], textarea'))
  .map(node => normalize('value' in node ? node.value : node.innerText || node.textContent || ''))
  .filter(Boolean);
const bodyValue = bodyCandidates.find(Boolean) || '';
const recipientCandidates = Array.from(document.querySelectorAll('[email], [data-hovercard-id], input[aria-label*="To"], textarea'))
  .map(node => normalize(node.getAttribute && (node.getAttribute('email') || node.getAttribute('data-hovercard-id')) || ('value' in node ? node.value : node.innerText || node.textContent || '')))
  .filter(Boolean);
const recipient = lower(expected.to || '');
const subject = normalize(expected.subject || '');
const body = normalize(expected.body || '');
return {
  host: location.host,
  href: location.href,
  title: document.title,
  is_gmail: location.host.includes('mail.google.com'),
  is_login_page: location.host.includes('accounts.google.com'),
  compose_open: Boolean(subjectInput) || lower(pageText).includes('send'),
  recipient_match: !recipient || recipientCandidates.some(candidate => lower(candidate).includes(recipient)) || lower(pageText).includes(recipient),
  subject_match: !subject || subjectValue === subject || pageText.includes(subject),
  body_match: !body || bodyValue.includes(body) || pageText.includes(body),
  subject_value: subjectValue,
  body_value: bodyValue,
  recipient_candidates: recipientCandidates.slice(0, 10),
  page_excerpt: pageText.slice(0, 500),
};
"#;

#[derive(Debug)]
struct DemoConfig {
    gmail_to: String,
    gmail_subject: String,
    gmail_body: String,
    message_to: String,
    message_body: String,
    artifacts_dir: PathBuf,
}

#[derive(Debug, Serialize)]
struct DemoReport {
    artifacts_dir: String,
    gmail: DemoStepReport,
    messages: DemoStepReport,
}

#[derive(Debug, Serialize)]
struct DemoStepReport {
    success: bool,
    summary: String,
    screenshot_path: Option<String>,
    details: Value,
}

#[tokio::main]
async fn main() -> Result<()> {
    logging::init();
    let config = parse_args()?;
    fs::create_dir_all(&config.artifacts_dir)
        .with_context(|| format!("failed to create {}", config.artifacts_dir.display()))?;

    println!("Artifacts: {}", config.artifacts_dir.display());

    let app_config = AppConfig::from_env();
    let automation = LocalAutomationBackend::from_config(&app_config);
    let browser = BrowserSidecarManager::new(app_config.browser.clone());

    let gmail = run_gmail_demo(&automation, &browser, &config).await?;
    let messages = run_messages_demo(&automation, &config).await?;
    let report = DemoReport {
        artifacts_dir: config.artifacts_dir.display().to_string(),
        gmail,
        messages,
    };

    let report_path = config.artifacts_dir.join("desktop-demo-report.json");
    fs::write(&report_path, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write {}", report_path.display()))?;

    println!("Report: {}", report_path.display());
    if report.gmail.success && report.messages.success {
        println!("Desktop demo passed.");
        Ok(())
    } else {
        bail!("desktop demo failed; see {}", report_path.display());
    }
}

async fn run_gmail_demo(
    automation: &LocalAutomationBackend,
    browser: &BrowserSidecarManager,
    config: &DemoConfig,
) -> Result<DemoStepReport> {
    println!("Step 1/2: opening Google Chrome and Gmail compose");
    run_tool(
        automation,
        "app_activate",
        json!({ "app_name": "Google Chrome" }),
    )
    .await
    .context("failed to activate Google Chrome")?;
    browser
        .call("/browser/attach_or_launch", json!({}))
        .await
        .context("failed to attach to or launch browser automation")?;
    run_tool(
        automation,
        "app_activate",
        json!({ "app_name": "Google Chrome" }),
    )
    .await
    .context("failed to bring Chrome frontmost after browser attach")?;

    browser
        .call(
            "/browser/open",
            json!({
                "url": gmail_compose_url(&config.gmail_to, &config.gmail_subject, &config.gmail_body),
            }),
        )
        .await
        .context("failed to open Gmail compose URL")?;

    let gmail_state = wait_for_gmail_state(browser, config).await?;
    let gmail_screenshot =
        capture_screenshot(automation, &config.artifacts_dir.join("gmail-compose.png"))
            .await
            .ok();

    let evaluation = gmail_state
        .get("evaluation")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let is_login_page = evaluation
        .get("is_login_page")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let passed = evaluation
        .get("is_gmail")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && !is_login_page
        && evaluation
            .get("compose_open")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && evaluation
            .get("recipient_match")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && evaluation
            .get("subject_match")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && evaluation
            .get("body_match")
            .and_then(Value::as_bool)
            .unwrap_or(false);

    let summary = if passed {
        format!("Verified Gmail compose draft for {}", config.gmail_to)
    } else if is_login_page {
        "Gmail compose did not open because the browser session is not signed in to Gmail"
            .to_string()
    } else {
        "Gmail compose did not verify all requested fields".to_string()
    };

    Ok(DemoStepReport {
        success: passed,
        summary,
        screenshot_path: gmail_screenshot.map(|path| path.display().to_string()),
        details: gmail_state,
    })
}

async fn run_messages_demo(
    automation: &LocalAutomationBackend,
    config: &DemoConfig,
) -> Result<DemoStepReport> {
    println!("Step 2/2: opening Messages draft");
    let result = run_tool(
        automation,
        "messages_compose",
        json!({
            "recipient": config.message_to,
            "body": config.message_body,
        }),
    )
    .await
    .context("failed to prepare Messages draft")?;

    let screenshot_path = capture_screenshot(
        automation,
        &config.artifacts_dir.join("messages-compose.png"),
    )
    .await
    .ok();
    let window_name = result
        .observation
        .get("window_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let recipient_verified = recipient_matches_window(window_name, &config.message_to);
    let passed = result.proof_passed && recipient_verified;
    let summary = if passed {
        format!("Verified Messages draft window for {}", config.message_to)
    } else {
        format!(
            "Messages draft opened, but the window title did not reliably match {}",
            config.message_to
        )
    };

    Ok(DemoStepReport {
        success: passed,
        summary,
        screenshot_path: screenshot_path.map(|path| path.display().to_string()),
        details: json!({
            "tool_summary": result.summary,
            "tool_observation": result.observation,
            "tool_output": result.output,
            "recipient_title_verified": recipient_verified,
        }),
    })
}

async fn run_tool(
    automation: &LocalAutomationBackend,
    name: &str,
    arguments: Value,
) -> Result<ToolCallResult> {
    automation
        .call_tool(ToolCallRequest {
            name: name.to_string(),
            arguments,
            risk: jarvis_rs::automation::RiskLevel::Low,
            requires_confirmation: false,
            target_identity: None,
            expected_outcome: None,
        })
        .await
}

async fn capture_screenshot(automation: &LocalAutomationBackend, path: &Path) -> Result<PathBuf> {
    run_tool(
        automation,
        "take_screenshot",
        json!({ "path": path.display().to_string() }),
    )
    .await
    .with_context(|| format!("failed to capture screenshot to {}", path.display()))?;
    Ok(path.to_path_buf())
}

async fn wait_for_gmail_state(
    browser: &BrowserSidecarManager,
    config: &DemoConfig,
) -> Result<Value> {
    let payload = json!({
        "script": GMAIL_INSPECTION_SCRIPT,
        "arg": {
            "to": config.gmail_to,
            "subject": config.gmail_subject,
            "body": config.gmail_body,
        }
    });

    for _ in 0..20 {
        let state = browser
            .call("/browser/eval", payload.clone())
            .await
            .context("failed to inspect Gmail compose state")?;
        let evaluation = state
            .get("evaluation")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let is_login_page = evaluation
            .get("is_login_page")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let compose_open = evaluation
            .get("compose_open")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if is_login_page || compose_open {
            return Ok(state);
        }
        sleep(Duration::from_millis(500)).await;
    }

    browser
        .call("/browser/eval", payload)
        .await
        .context("failed to inspect Gmail compose state after retries")
}

fn gmail_compose_url(to: &str, subject: &str, body: &str) -> String {
    format!(
        "https://mail.google.com/mail/?view=cm&fs=1&tf=1&to={}&su={}&body={}",
        percent_encode_for_url(to),
        percent_encode_for_url(subject),
        percent_encode_for_url(body)
    )
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

fn recipient_matches_window(window_name: &str, recipient: &str) -> bool {
    let normalized_window = normalize_for_match(window_name);
    let normalized_recipient = normalize_for_match(recipient);
    if normalized_window.is_empty() || normalized_recipient.is_empty() {
        return false;
    }

    if normalized_window.contains(&normalized_recipient) {
        return true;
    }

    let recipient_digits: String = recipient.chars().filter(|ch| ch.is_ascii_digit()).collect();
    let window_digits: String = window_name
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect();
    recipient_digits.len() >= 4
        && window_digits.len() >= 4
        && window_digits.ends_with(&recipient_digits[recipient_digits.len() - 4..])
}

fn normalize_for_match(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '@' || *ch == '.')
        .collect()
}

fn parse_args() -> Result<DemoConfig> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage();
        std::process::exit(0);
    }

    let mut gmail_to = None;
    let mut gmail_subject = None;
    let mut gmail_body = None;
    let mut message_to = None;
    let mut message_body = None;
    let mut artifacts_dir = None;

    let mut index = 0;
    while index < args.len() {
        let flag = &args[index];
        let value = args
            .get(index + 1)
            .ok_or_else(|| anyhow!("missing value for {}", flag))?;
        match flag.as_str() {
            "--gmail-to" => gmail_to = Some(value.clone()),
            "--gmail-subject" => gmail_subject = Some(value.clone()),
            "--gmail-body" => gmail_body = Some(value.clone()),
            "--message-to" => message_to = Some(value.clone()),
            "--message-body" => message_body = Some(value.clone()),
            "--artifacts-dir" => artifacts_dir = Some(PathBuf::from(value)),
            _ => bail!("unknown argument '{}'", flag),
        }
        index += 2;
    }

    Ok(DemoConfig {
        gmail_to: gmail_to.ok_or_else(|| anyhow!("--gmail-to is required"))?,
        gmail_subject: gmail_subject.ok_or_else(|| anyhow!("--gmail-subject is required"))?,
        gmail_body: gmail_body.ok_or_else(|| anyhow!("--gmail-body is required"))?,
        message_to: message_to.ok_or_else(|| anyhow!("--message-to is required"))?,
        message_body: message_body.ok_or_else(|| anyhow!("--message-body is required"))?,
        artifacts_dir: artifacts_dir.unwrap_or_else(default_artifacts_dir),
    })
}

fn default_artifacts_dir() -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".tooling")
        .join("desktop-demo")
        .join(format!("{}", millis))
}

fn print_usage() {
    println!(
        "Usage: cargo run -p jarvis_rs --bin desktop_demo -- \
  --gmail-to person@example.com \
  --gmail-subject \"Demo subject\" \
  --gmail-body \"Demo email body\" \
  --message-to \"+15551234567\" \
  --message-body \"Demo iMessage body\" \
  [--artifacts-dir /absolute/path]"
    );
    println!("The demo stops at visible drafts. It does not send email or messages.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmail_compose_url_encodes_expected_fields() {
        let url = gmail_compose_url("demo@example.com", "Hello there", "Line 1 & 2");
        assert!(url.contains("to=demo%40example.com"));
        assert!(url.contains("su=Hello%20there"));
        assert!(url.contains("body=Line%201%20%26%202"));
    }

    #[test]
    fn recipient_match_accepts_phone_suffix() {
        assert!(recipient_matches_window(
            "Chat with +1 (555) 123-4567",
            "+15551234567"
        ));
    }
}
