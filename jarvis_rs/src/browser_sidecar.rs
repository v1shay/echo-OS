use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::BrowserConfig;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BrowserSidecarDiagnostics {
    pub endpoint: String,
    pub node_ready: bool,
    pub npm_ready: bool,
    pub chrome_ready: bool,
    pub sidecar_files_ready: bool,
    pub playwright_installed: bool,
    pub sidecar_running: bool,
    #[serde(default)]
    pub browser_mode: Option<String>,
    #[serde(default)]
    pub setup_items: Vec<String>,
}

pub struct BrowserSidecarManager {
    config: BrowserConfig,
    http: Client,
    child: Mutex<Option<Child>>,
}

impl std::fmt::Debug for BrowserSidecarManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrowserSidecarManager")
            .field("endpoint", &self.config.sidecar_endpoint)
            .field("sidecar_dir", &self.config.sidecar_dir)
            .field("sidecar_entry", &self.config.sidecar_entry)
            .finish()
    }
}

impl BrowserSidecarManager {
    pub fn new(config: BrowserConfig) -> Self {
        Self {
            config,
            http: Client::new(),
            child: Mutex::new(None),
        }
    }

    pub fn diagnostics(&self) -> BrowserSidecarDiagnostics {
        let node_ready = self.config.node_binary.exists();
        let npm_ready = self.config.npm_binary.exists();
        let chrome_ready = self.config.chrome_executable.exists();
        let sidecar_files_ready =
            self.config.sidecar_dir.exists() && self.config.sidecar_entry.exists();
        let playwright_installed = self
            .config
            .sidecar_dir
            .join("node_modules/playwright-core")
            .exists();
        let health = self.health_sync_value();
        let sidecar_running = health
            .as_ref()
            .map(Self::payload_is_healthy)
            .unwrap_or(false);
        let browser_mode = health
            .as_ref()
            .and_then(|value| value.get("mode"))
            .and_then(Value::as_str)
            .map(str::to_string);

        let mut setup_items = Vec::new();
        if !node_ready {
            setup_items.push(format!(
                "Local Node runtime is missing at {}",
                self.config.node_binary.display()
            ));
        }
        if !playwright_installed {
            setup_items.push(format!(
                "Playwright sidecar dependency is missing in {}",
                self.config.sidecar_dir.display()
            ));
        }
        if !chrome_ready {
            setup_items.push(format!(
                "Google Chrome executable is missing at {}",
                self.config.chrome_executable.display()
            ));
        }
        if !sidecar_running {
            setup_items.push(
                "Browser sidecar is not running yet; it will be started on the first browser task"
                    .to_string(),
            );
        }

        BrowserSidecarDiagnostics {
            endpoint: self.config.sidecar_endpoint.clone(),
            node_ready,
            npm_ready,
            chrome_ready,
            sidecar_files_ready,
            playwright_installed,
            sidecar_running,
            browser_mode,
            setup_items,
        }
    }

    pub async fn ensure_running(&self) -> Result<Value> {
        if let Some(health) = self.health().await? {
            return Ok(health);
        }

        if let Some(recovered) = self.recover_existing_sidecar().await? {
            return Ok(recovered);
        }

        self.bootstrap_prerequisites()?;
        self.cleanup_stale_listener()?;
        self.spawn_sidecar_if_needed()?;

        for _ in 0..30 {
            if let Some(health) = self.health().await? {
                return Ok(health);
            }
            if let Some(recovered) = self.recover_existing_sidecar().await? {
                return Ok(recovered);
            }
            thread::sleep(Duration::from_millis(200));
        }

        bail!(
            "browser sidecar did not become healthy at {}",
            self.config.sidecar_endpoint
        )
    }

    pub async fn call(&self, route: &str, payload: Value) -> Result<Value> {
        self.ensure_running().await?;
        let url = format!(
            "{}/{}",
            self.config.sidecar_endpoint.trim_end_matches('/'),
            route.trim_start_matches('/')
        );
        let response = self.http.post(url).json(&payload).send().await?;
        let status = response.status();
        let value: Value = response.json().await?;
        if !status.is_success() || !Self::payload_is_healthy(&value) {
            let message = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown browser automation error");
            bail!("{}", message);
        }
        Ok(value)
    }

    fn bootstrap_prerequisites(&self) -> Result<()> {
        if !self.config.node_binary.exists() {
            bail!(
                "local Node runtime is missing at {}",
                self.config.node_binary.display()
            );
        }
        if !self.config.npm_binary.exists() {
            bail!(
                "local npm runtime is missing at {}",
                self.config.npm_binary.display()
            );
        }
        if !self.config.sidecar_entry.exists() {
            bail!(
                "browser sidecar entry is missing at {}",
                self.config.sidecar_entry.display()
            );
        }
        if !self
            .config
            .sidecar_dir
            .join("node_modules/playwright-core")
            .exists()
        {
            bail!(
                "Playwright sidecar dependency is missing. Install it in {} before browser tasks can run.",
                self.config.sidecar_dir.display()
            );
        }
        Ok(())
    }

    fn spawn_sidecar_if_needed(&self) -> Result<()> {
        let mut child_guard = self
            .child
            .lock()
            .map_err(|_| anyhow!("browser sidecar child mutex poisoned"))?;

        if let Some(existing) = child_guard.as_mut() {
            if existing.try_wait()?.is_none() {
                return Ok(());
            }
            *child_guard = None;
        }

        std::fs::create_dir_all(&self.config.dedicated_profile_dir).with_context(|| {
            format!(
                "failed to create browser profile directory {}",
                self.config.dedicated_profile_dir.display()
            )
        })?;

        let child = Command::new(&self.config.node_binary)
            .arg(&self.config.sidecar_entry)
            .current_dir(&self.config.sidecar_dir)
            .env("JARVIS_BROWSER_HOST", &self.config.sidecar_host)
            .env("JARVIS_BROWSER_PORT", self.config.sidecar_port.to_string())
            .env(
                "JARVIS_CHROME_EXECUTABLE",
                self.config.chrome_executable.display().to_string(),
            )
            .env(
                "JARVIS_CHROME_DEBUG_PORT",
                self.config.chrome_debug_port.to_string(),
            )
            .env("JARVIS_CHROME_ATTACH_URL", &self.config.chrome_attach_url)
            .env(
                "JARVIS_BROWSER_PROFILE_DIR",
                self.config.dedicated_profile_dir.display().to_string(),
            )
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| {
                format!(
                    "failed to start browser sidecar from {}",
                    self.config.sidecar_entry.display()
                )
            })?;

        *child_guard = Some(child);
        Ok(())
    }

    fn cleanup_stale_listener(&self) -> Result<()> {
        let port_arg = format!("-iTCP:{}", self.config.sidecar_port);
        let output = match Command::new("lsof")
            .arg("-t")
            .arg("-nP")
            .arg(port_arg)
            .arg("-sTCP:LISTEN")
            .output()
        {
            Ok(output) => output,
            Err(_) => return Ok(()),
        };

        if !output.status.success() {
            return Ok(());
        }

        let mut child_guard = self
            .child
            .lock()
            .map_err(|_| anyhow!("browser sidecar child mutex poisoned"))?;

        for pid in String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|pid| !pid.is_empty())
        {
            if let Some(existing) = child_guard.as_mut() {
                if existing.id().to_string() == pid {
                    let _ = existing.kill();
                    let _ = existing.wait();
                    *child_guard = None;
                    continue;
                }
            }

            let _ = Command::new("kill").arg("-TERM").arg(pid).status();
        }

        thread::sleep(Duration::from_millis(250));
        Ok(())
    }

    async fn health(&self) -> Result<Option<Value>> {
        let url = format!(
            "{}/health",
            self.config.sidecar_endpoint.trim_end_matches('/')
        );
        match self.http.get(url).send().await {
            Ok(response) if response.status().is_success() => {
                let value = response.json::<Value>().await?;
                if Self::payload_is_healthy(&value) {
                    Ok(Some(value))
                } else {
                    Ok(None)
                }
            }
            Ok(_) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    async fn recover_existing_sidecar(&self) -> Result<Option<Value>> {
        let url = format!(
            "{}/browser/reset",
            self.config.sidecar_endpoint.trim_end_matches('/')
        );
        match self.http.post(url).json(&json!({})).send().await {
            Ok(response) if response.status().is_success() => {
                let value = response.json::<Value>().await?;
                if Self::payload_is_healthy(&value) {
                    Ok(Some(value))
                } else {
                    Ok(None)
                }
            }
            Ok(_) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    fn health_sync_value(&self) -> Option<Value> {
        let output = Command::new("curl")
            .arg("-sf")
            .arg(format!(
                "{}/health",
                self.config.sidecar_endpoint.trim_end_matches('/')
            ))
            .status()
            .ok()?;
        if !output.success() {
            return None;
        }
        let output = Command::new("curl")
            .arg("-sf")
            .arg(format!(
                "{}/health",
                self.config.sidecar_endpoint.trim_end_matches('/')
            ))
            .output()
            .ok()?;
        serde_json::from_slice(&output.stdout).ok()
    }

    fn payload_is_healthy(value: &Value) -> bool {
        value.get("ok").and_then(Value::as_bool).unwrap_or(true)
    }

    pub fn config_paths(&self) -> Vec<PathBuf> {
        vec![
            self.config.sidecar_dir.clone(),
            self.config.sidecar_entry.clone(),
            self.config.node_binary.clone(),
            self.config.npm_binary.clone(),
            self.config.chrome_executable.clone(),
            self.config.dedicated_profile_dir.clone(),
        ]
    }

    pub fn endpoint(&self) -> &str {
        &self.config.sidecar_endpoint
    }
}

pub fn browser_target_from_value(value: &Value) -> Value {
    json!({
        "browser_name": value.get("browserName").and_then(Value::as_str).unwrap_or("Google Chrome"),
        "url": value.get("url").and_then(Value::as_str),
        "tab_hint": value.get("title").and_then(Value::as_str),
    })
}
