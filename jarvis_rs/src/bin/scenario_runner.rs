use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use jarvis_rs::agent_core::{
    build_runtime_manifest, RunReport, ScenarioOutcome, VerificationResult,
};
use jarvis_rs::automation::{
    AutomationBackend, LocalAutomationBackend, RiskLevel, ToolCallRequest, ToolCallResult,
};
use jarvis_rs::browser_sidecar::BrowserSidecarManager;
use jarvis_rs::config::AppConfig;
use jarvis_rs::llm::provider::OpenAiCompatibleClient;
use jarvis_rs::logging;

#[tokio::main]
async fn main() -> Result<()> {
    logging::init();

    let artifacts_dir = artifacts_dir()?;
    fs::create_dir_all(&artifacts_dir)
        .with_context(|| format!("failed to create {}", artifacts_dir.display()))?;

    let config = AppConfig::from_env();
    let automation = LocalAutomationBackend::from_config(&config);
    let browser = BrowserSidecarManager::new(config.browser.clone());
    let capabilities = automation.capabilities();
    let runtime = build_runtime_manifest(
        &config,
        &capabilities,
        &capabilities.setup_items,
        "scenario-runner",
        "scenario-runner",
    );

    let mut scenarios = Vec::new();
    scenarios.push(run_app_focus_task(&automation, &artifacts_dir).await?);
    scenarios.push(run_browser_task(&automation, &browser, &artifacts_dir).await?);
    scenarios.push(run_filesystem_task(&automation, &config, &artifacts_dir).await?);
    scenarios.push(run_mail_draft_task(&automation, &config, &artifacts_dir).await?);
    scenarios.push(run_multi_app_task(&automation, &browser, &config, &artifacts_dir).await?);

    let report = RunReport { runtime, scenarios };
    let report_path = artifacts_dir.join("scenario-run-report.json");
    fs::write(&report_path, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write {}", report_path.display()))?;

    println!("Scenario artifacts: {}", artifacts_dir.display());
    println!("Scenario report: {}", report_path.display());

    if report.scenarios.iter().all(|scenario| scenario.success) {
        println!("All scenario tasks passed.");
        Ok(())
    } else {
        bail!(
            "one or more scenario tasks failed; see {}",
            report_path.display()
        );
    }
}

async fn run_app_focus_task(
    automation: &LocalAutomationBackend,
    artifacts_dir: &Path,
) -> Result<ScenarioOutcome> {
    let activate = run_tool(automation, "app_activate", json!({ "app_name": "Finder" })).await;
    let snapshot = run_tool(automation, "window_snapshot", json!({})).await;
    let screenshot = capture_screenshot(automation, &artifacts_dir.join("app-focus-finder.png"))
        .await
        .ok();

    let success = activate
        .as_ref()
        .map(|result| result.proof_passed)
        .unwrap_or(false)
        && tool_observation_str(&snapshot, "app_name")
            .map(|value| value == "Finder")
            .unwrap_or(false);
    let verification = verification(
        success,
        "Finder became the frontmost application",
        "Finder was not confirmed as the frontmost application",
    );
    let detail_path = write_detail(
        artifacts_dir,
        "app-focus-finder.json",
        &json!({
        "activate": result_json(&activate),
        "snapshot": result_json(&snapshot),
        "verification": verification.clone(),
        }),
    )?;

    Ok(ScenarioOutcome {
        id: "app_focus_finder".to_string(),
        success,
        summary: verification.summary.clone(),
        verification: Some(verification),
        artifact_paths: artifact_paths(&[Some(detail_path), screenshot]),
    })
}

async fn run_browser_task(
    automation: &LocalAutomationBackend,
    browser: &BrowserSidecarManager,
    artifacts_dir: &Path,
) -> Result<ScenarioOutcome> {
    let activate = run_tool(
        automation,
        "app_activate",
        json!({ "app_name": "Google Chrome" }),
    )
    .await;
    let attach = browser.call("/browser/attach_or_launch", json!({})).await;
    let open = browser
        .call("/browser/open", json!({ "url": "https://example.com" }))
        .await;
    let assertion = browser
        .call(
            "/browser/assert",
            json!({ "title_contains": "Example Domain", "text_contains": "Example Domain" }),
        )
        .await;
    let screenshot = capture_screenshot(automation, &artifacts_dir.join("browser-example.png"))
        .await
        .ok();

    let success = activate
        .as_ref()
        .map(|result| result.proof_passed)
        .unwrap_or(false)
        && attach
            .as_ref()
            .map(|value| value["ok"] == true)
            .unwrap_or(false)
        && open
            .as_ref()
            .map(|value| value["ok"] == true)
            .unwrap_or(false)
        && value_bool(&assertion, "matched").unwrap_or(false);
    let verification = verification(
        success,
        "Browser workflow opened Example Domain and verified visible state",
        "Browser workflow did not verify the requested page state",
    );
    let detail_path = write_detail(
        artifacts_dir,
        "browser-example.json",
        &json!({
            "activate": result_json(&activate),
            "attach": value_json(&attach),
            "open": value_json(&open),
            "assertion": value_json(&assertion),
            "verification": verification.clone(),
        }),
    )?;

    Ok(ScenarioOutcome {
        id: "browser_example_domain".to_string(),
        success,
        summary: verification.summary.clone(),
        verification: Some(verification),
        artifact_paths: artifact_paths(&[Some(detail_path), screenshot]),
    })
}

async fn run_filesystem_task(
    automation: &LocalAutomationBackend,
    config: &AppConfig,
    artifacts_dir: &Path,
) -> Result<ScenarioOutcome> {
    let work_dir = artifacts_dir.join("filesystem-roundtrip");
    let source_path = work_dir.join("generated-note.txt");
    let moved_path = work_dir.join("moved-note.txt");
    let (generated_text, generator) = generate_task_text(
        config,
        "Write a concise two-sentence status note about proving an autonomous filesystem workflow.",
    )
    .await;

    let create = run_tool(
        automation,
        "filesystem_create_folder",
        json!({ "path": work_dir.display().to_string() }),
    )
    .await;
    let write = run_tool(
        automation,
        "filesystem_write_file",
        json!({
            "path": source_path.display().to_string(),
            "contents": generated_text,
        }),
    )
    .await;
    let read = run_tool(
        automation,
        "filesystem_read_file",
        json!({ "path": source_path.display().to_string() }),
    )
    .await;
    let mv = run_tool(
        automation,
        "filesystem_move",
        json!({
            "from": source_path.display().to_string(),
            "to": moved_path.display().to_string(),
        }),
    )
    .await;
    let list = run_tool(
        automation,
        "filesystem_list",
        json!({ "path": work_dir.display().to_string() }),
    )
    .await;

    let success = create.is_ok()
        && write.is_ok()
        && tool_output(&read)
            .map(|output| !output.trim().is_empty())
            .unwrap_or(false)
        && mv.is_ok()
        && tool_output(&list)
            .map(|output| output.contains("moved-note.txt"))
            .unwrap_or(false);
    let verification = verification(
        success,
        "Filesystem roundtrip created, read, moved, and listed generated content",
        "Filesystem roundtrip did not complete every required step",
    );
    let detail_path = write_detail(
        artifacts_dir,
        "filesystem-roundtrip.json",
        &json!({
            "generator": generator,
            "create": result_json(&create),
            "write": result_json(&write),
            "read": result_json(&read),
            "move": result_json(&mv),
            "list": result_json(&list),
            "verification": verification.clone(),
        }),
    )?;

    Ok(ScenarioOutcome {
        id: "filesystem_roundtrip".to_string(),
        success,
        summary: verification.summary.clone(),
        verification: Some(verification),
        artifact_paths: artifact_paths(&[Some(detail_path)]),
    })
}

async fn run_mail_draft_task(
    automation: &LocalAutomationBackend,
    config: &AppConfig,
    artifacts_dir: &Path,
) -> Result<ScenarioOutcome> {
    let (generated_body, generator) = generate_task_text(
        config,
        "Write a short professional email body that says Jarvis completed a no-help autonomy validation run.",
    )
    .await;
    let result = run_tool(
        automation,
        "mail_compose",
        json!({
            "to": Option::<String>::None,
            "subject": "Jarvis autonomy validation draft",
            "body": generated_body,
        }),
    )
    .await;
    let screenshot = capture_screenshot(automation, &artifacts_dir.join("mail-draft.png"))
        .await
        .ok();
    let success = result
        .as_ref()
        .map(|value| value.proof_passed)
        .unwrap_or(false);
    let verification = verification(
        success,
        "Mail draft with generated text was created and verified",
        "Mail draft generation did not verify successfully",
    );
    let detail_path = write_detail(
        artifacts_dir,
        "mail-draft.json",
        &json!({
            "generator": generator,
            "result": result_json(&result),
            "verification": verification.clone(),
        }),
    )?;

    Ok(ScenarioOutcome {
        id: "mail_generated_draft".to_string(),
        success,
        summary: verification.summary.clone(),
        verification: Some(verification),
        artifact_paths: artifact_paths(&[Some(detail_path), screenshot]),
    })
}

async fn run_multi_app_task(
    automation: &LocalAutomationBackend,
    browser: &BrowserSidecarManager,
    config: &AppConfig,
    artifacts_dir: &Path,
) -> Result<ScenarioOutcome> {
    let summary_file = artifacts_dir.join("multi-app-summary.txt");
    let browser_open = browser
        .call("/browser/open", json!({ "url": "https://example.com" }))
        .await;
    let extracted = browser
        .call("/browser/extract_text", json!({ "max_chars": 400 }))
        .await;
    let extracted_text =
        value_str(&extracted, "extractedText").unwrap_or_else(|| "Example Domain".to_string());
    let prompt = format!(
        "Summarize this web page in two concise sentences for an autonomy regression note:\n\n{}",
        extracted_text
    );
    let (generated_summary, generator) = generate_task_text(config, &prompt).await;
    let write = run_tool(
        automation,
        "filesystem_write_file",
        json!({
            "path": summary_file.display().to_string(),
            "contents": generated_summary.clone(),
        }),
    )
    .await;
    let mail = run_tool(
        automation,
        "mail_compose",
        json!({
            "to": Option::<String>::None,
            "subject": "Multi-app autonomy summary",
            "body": generated_summary,
        }),
    )
    .await;
    let screenshot = capture_screenshot(automation, &artifacts_dir.join("multi-app-mail.png"))
        .await
        .ok();

    let success = browser_open
        .as_ref()
        .map(|value| value["ok"] == true)
        .unwrap_or(false)
        && value_str(&extracted, "extractedText")
            .map(|text| !text.trim().is_empty())
            .unwrap_or(false)
        && write.is_ok()
        && mail
            .as_ref()
            .map(|value| value.proof_passed)
            .unwrap_or(false);
    let verification = verification(
        success,
        "Multi-app workflow crossed browser, filesystem, and Mail with generated output",
        "Multi-app workflow did not verify across every subsystem",
    );
    let detail_path = write_detail(
        artifacts_dir,
        "multi-app-summary.json",
        &json!({
            "generator": generator,
            "browser_open": value_json(&browser_open),
            "extracted": value_json(&extracted),
            "write": result_json(&write),
            "mail": result_json(&mail),
            "verification": verification.clone(),
        }),
    )?;

    Ok(ScenarioOutcome {
        id: "multi_app_browser_file_mail".to_string(),
        success,
        summary: verification.summary.clone(),
        verification: Some(verification),
        artifact_paths: artifact_paths(&[Some(detail_path), screenshot]),
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
            risk: RiskLevel::Low,
            requires_confirmation: false,
            target_identity: None,
            expected_outcome: None,
        })
        .await
}

async fn capture_screenshot(automation: &LocalAutomationBackend, path: &Path) -> Result<PathBuf> {
    let _ = run_tool(
        automation,
        "take_screenshot",
        json!({ "path": path.display().to_string() }),
    )
    .await?;
    if path.exists() {
        Ok(path.to_path_buf())
    } else {
        bail!(
            "screenshot command returned without creating {}",
            path.display()
        );
    }
}

async fn generate_task_text(config: &AppConfig, prompt: &str) -> (String, String) {
    let mut clients = Vec::new();
    if let (Some(base_url), Some(model)) = (
        config.provider.fallback_base_url.clone(),
        config.provider.fallback_model.clone(),
    ) {
        clients.push((
            "hosted-openai".to_string(),
            OpenAiCompatibleClient::new(base_url, model, config.provider.fallback_api_key.clone()),
        ));
    }
    clients.push((
        "local-worker".to_string(),
        OpenAiCompatibleClient::new(
            config.provider.worker_endpoint.clone(),
            config.provider.worker_model.clone(),
            config.provider.fallback_api_key.clone(),
        ),
    ));

    for (label, client) in clients {
        match client
            .complete_text(
                "You write concise desktop-task output. Return plain text only.",
                prompt,
            )
            .await
        {
            Ok(text) if !text.trim().is_empty() => return (text.trim().to_string(), label),
            _ => continue,
        }
    }

    let fallback = prompt
        .split_whitespace()
        .take(24)
        .collect::<Vec<_>>()
        .join(" ");
    (
        format!("Fallback generated text: {}", fallback),
        "deterministic-fallback".to_string(),
    )
}

fn artifacts_dir() -> Result<PathBuf> {
    let current_dir = std::env::current_dir().context("failed to read current directory")?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock drifted before unix epoch")?
        .as_secs();
    Ok(current_dir
        .join("scenario_artifacts")
        .join(format!("run-{}", stamp)))
}

fn write_detail(artifacts_dir: &Path, file_name: &str, payload: &Value) -> Result<PathBuf> {
    let path = artifacts_dir.join(file_name);
    fs::write(&path, serde_json::to_string_pretty(payload)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

fn artifact_paths(values: &[Option<PathBuf>]) -> Vec<String> {
    values
        .iter()
        .flatten()
        .map(|path| path.display().to_string())
        .collect()
}

fn verification(success: bool, success_summary: &str, failure_summary: &str) -> VerificationResult {
    VerificationResult {
        satisfied: success,
        confidence: if success { 0.95 } else { 0.2 },
        summary: if success {
            success_summary.to_string()
        } else {
            failure_summary.to_string()
        },
        matched_criteria: if success {
            vec![success_summary.to_string()]
        } else {
            Vec::new()
        },
        missing_criteria: if success {
            Vec::new()
        } else {
            vec![failure_summary.to_string()]
        },
    }
}

fn result_json(result: &Result<ToolCallResult>) -> Value {
    match result {
        Ok(value) => {
            serde_json::to_value(value).unwrap_or_else(|_| json!({"error": "serialization failed"}))
        }
        Err(error) => json!({ "error": error.to_string() }),
    }
}

fn value_json(result: &Result<Value>) -> Value {
    match result {
        Ok(value) => value.clone(),
        Err(error) => json!({ "error": error.to_string() }),
    }
}

fn tool_output(result: &Result<ToolCallResult>) -> Option<String> {
    result.as_ref().ok().and_then(|value| value.output.clone())
}

fn tool_observation_str(result: &Result<ToolCallResult>, field: &str) -> Option<String> {
    result
        .as_ref()
        .ok()
        .and_then(|value| value.observation.get(field))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn value_str(result: &Result<Value>, field: &str) -> Option<String> {
    result
        .as_ref()
        .ok()
        .and_then(|value| value.get(field))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn value_bool(result: &Result<Value>, field: &str) -> Option<bool> {
    result
        .as_ref()
        .ok()
        .and_then(|value| value.get(field))
        .and_then(Value::as_bool)
}
