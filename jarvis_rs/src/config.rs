use std::env;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub llama_server_binary: String,
    pub planner_endpoint: String,
    pub planner_model: String,
    pub planner_model_path: Option<PathBuf>,
    pub worker_endpoint: String,
    pub worker_model: String,
    pub worker_model_path: Option<PathBuf>,
    pub fallback_base_url: Option<String>,
    pub fallback_model: Option<String>,
    pub fallback_api_key: Option<String>,
    pub model_context_size: usize,
}

#[derive(Clone, Debug)]
pub struct SmsConfig {
    pub account_sid: Option<String>,
    pub auth_token: Option<String>,
    pub from_number: Option<String>,
    pub to_number: Option<String>,
    pub webhook_bind: String,
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub provider: ProviderConfig,
    pub sms: SmsConfig,
    pub whisper_model_path: Option<PathBuf>,
    pub recording_path: PathBuf,
    pub allowed_paths: Vec<PathBuf>,
    pub capture_seconds: u64,
    pub voice_name: String,
    pub primary_browser: String,
    pub max_worker_iterations: usize,
    pub browser: BrowserConfig,
}

#[derive(Clone, Debug)]
pub struct BrowserConfig {
    pub sidecar_endpoint: String,
    pub sidecar_host: String,
    pub sidecar_port: u16,
    pub sidecar_dir: PathBuf,
    pub sidecar_entry: PathBuf,
    pub node_binary: PathBuf,
    pub npm_binary: PathBuf,
    pub chrome_executable: PathBuf,
    pub chrome_debug_port: u16,
    pub chrome_attach_url: String,
    pub dedicated_profile_dir: PathBuf,
}

impl Default for AppConfig {
    fn default() -> Self {
        let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let home_dir = env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| current_dir.clone());
        let project_root = detect_project_root().unwrap_or_else(|| current_dir.clone());
        let default_model_path = auto_detect_llama_model();
        let local_node_root = project_root.join(".tooling/node-v24.14.0-darwin-arm64");
        let browser_sidecar_dir = project_root.join("browser_sidecar");
        let browser_port = 4317;
        let chrome_debug_port = 9222;

        let mut allowed_paths = vec![current_dir.clone()];
        allowed_paths.push(home_dir.join("Desktop"));
        allowed_paths.push(home_dir.join("Documents"));
        allowed_paths.push(home_dir.join("Downloads"));

        Self {
            provider: ProviderConfig {
                llama_server_binary: "llama-server".to_string(),
                planner_endpoint: "http://127.0.0.1:8012/v1".to_string(),
                planner_model: "jarvis-planner".to_string(),
                planner_model_path: default_model_path.clone(),
                worker_endpoint: "http://127.0.0.1:8013/v1".to_string(),
                worker_model: "jarvis-worker".to_string(),
                worker_model_path: default_model_path,
                fallback_base_url: None,
                fallback_model: None,
                fallback_api_key: None,
                model_context_size: 4096,
            },
            sms: SmsConfig {
                account_sid: None,
                auth_token: None,
                from_number: None,
                to_number: None,
                webhook_bind: "127.0.0.1:8787".to_string(),
            },
            whisper_model_path: None,
            recording_path: env::temp_dir().join("jarvis-input.wav"),
            allowed_paths,
            capture_seconds: 5,
            voice_name: "Samantha".to_string(),
            primary_browser: "Google Chrome".to_string(),
            max_worker_iterations: 16,
            browser: BrowserConfig {
                sidecar_endpoint: format!("http://127.0.0.1:{}", browser_port),
                sidecar_host: "127.0.0.1".to_string(),
                sidecar_port: browser_port,
                sidecar_dir: browser_sidecar_dir.clone(),
                sidecar_entry: browser_sidecar_dir.join("server.mjs"),
                node_binary: local_node_root.join("bin/node"),
                npm_binary: local_node_root.join("bin/npm"),
                chrome_executable: PathBuf::from(
                    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                ),
                chrome_debug_port,
                chrome_attach_url: format!("http://127.0.0.1:{}", chrome_debug_port),
                dedicated_profile_dir: project_root.join(".tooling/browser-profile"),
            },
        }
    }
}

impl AppConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(value) = env::var("JARVIS_LLAMA_SERVER_BINARY") {
            config.provider.llama_server_binary = value;
        }
        if let Ok(value) = env::var("JARVIS_PLANNER_ENDPOINT") {
            config.provider.planner_endpoint = value;
        }
        if let Ok(value) = env::var("JARVIS_PLANNER_MODEL") {
            config.provider.planner_model = value;
        }
        if let Ok(value) = env::var("JARVIS_PLANNER_MODEL_PATH") {
            config.provider.planner_model_path = Some(PathBuf::from(value));
        }
        if let Ok(value) = env::var("JARVIS_WORKER_ENDPOINT") {
            config.provider.worker_endpoint = value;
        }
        if let Ok(value) = env::var("JARVIS_WORKER_MODEL") {
            config.provider.worker_model = value;
        }
        if let Ok(value) = env::var("JARVIS_WORKER_MODEL_PATH") {
            config.provider.worker_model_path = Some(PathBuf::from(value));
        }
        if let Ok(value) = env::var("JARVIS_FALLBACK_BASE_URL") {
            config.provider.fallback_base_url = Some(value);
        }
        if let Ok(value) = env::var("JARVIS_FALLBACK_MODEL") {
            config.provider.fallback_model = Some(value);
        }
        if let Ok(value) = env::var("JARVIS_FALLBACK_API_KEY") {
            config.provider.fallback_api_key = Some(value);
        }
        if let Ok(value) = env::var("JARVIS_MODEL_CONTEXT_SIZE") {
            if let Ok(parsed) = value.parse::<usize>() {
                config.provider.model_context_size = parsed.max(1024);
            }
        }
        if let Ok(value) = env::var("JARVIS_SMS_ACCOUNT_SID") {
            config.sms.account_sid = Some(value);
        }
        if let Ok(value) = env::var("JARVIS_SMS_AUTH_TOKEN") {
            config.sms.auth_token = Some(value);
        }
        if let Ok(value) = env::var("JARVIS_SMS_FROM_NUMBER") {
            config.sms.from_number = Some(value);
        }
        if let Ok(value) = env::var("JARVIS_SMS_TO_NUMBER") {
            config.sms.to_number = Some(value);
        }
        if let Ok(value) = env::var("JARVIS_SMS_WEBHOOK_BIND") {
            config.sms.webhook_bind = value;
        }
        if let Ok(value) = env::var("JARVIS_WHISPER_MODEL_PATH") {
            config.whisper_model_path = Some(PathBuf::from(value));
        } else if let Some(path) = auto_detect_whisper_model() {
            config.whisper_model_path = Some(path);
        }
        if let Ok(value) = env::var("JARVIS_RECORDING_PATH") {
            config.recording_path = PathBuf::from(value);
        }
        if let Ok(value) = env::var("JARVIS_CAPTURE_SECONDS") {
            if let Ok(seconds) = value.parse::<u64>() {
                config.capture_seconds = seconds.max(1);
            }
        }
        if let Ok(value) = env::var("JARVIS_VOICE_NAME") {
            config.voice_name = value;
        }
        if let Ok(value) = env::var("JARVIS_PRIMARY_BROWSER") {
            config.primary_browser = value;
        }
        if let Ok(value) = env::var("JARVIS_MAX_WORKER_ITERATIONS") {
            if let Ok(parsed) = value.parse::<usize>() {
                config.max_worker_iterations = parsed.max(4);
            }
        }
        if let Ok(value) = env::var("JARVIS_BROWSER_SIDECAR_ENDPOINT") {
            config.browser.sidecar_endpoint = value;
        }
        if let Ok(value) = env::var("JARVIS_BROWSER_SIDECAR_HOST") {
            config.browser.sidecar_host = value;
        }
        if let Ok(value) = env::var("JARVIS_BROWSER_SIDECAR_PORT") {
            if let Ok(parsed) = value.parse::<u16>() {
                config.browser.sidecar_port = parsed;
            }
        }
        if let Ok(value) = env::var("JARVIS_BROWSER_SIDECAR_DIR") {
            config.browser.sidecar_dir = PathBuf::from(value);
        }
        if let Ok(value) = env::var("JARVIS_BROWSER_SIDECAR_ENTRY") {
            config.browser.sidecar_entry = PathBuf::from(value);
        }
        if let Ok(value) = env::var("JARVIS_NODE_BINARY") {
            config.browser.node_binary = PathBuf::from(value);
        }
        if let Ok(value) = env::var("JARVIS_NPM_BINARY") {
            config.browser.npm_binary = PathBuf::from(value);
        }
        if let Ok(value) = env::var("JARVIS_CHROME_EXECUTABLE") {
            config.browser.chrome_executable = PathBuf::from(value);
        }
        if let Ok(value) = env::var("JARVIS_CHROME_DEBUG_PORT") {
            if let Ok(parsed) = value.parse::<u16>() {
                config.browser.chrome_debug_port = parsed;
            }
        }
        if let Ok(value) = env::var("JARVIS_CHROME_ATTACH_URL") {
            config.browser.chrome_attach_url = value;
        }
        if let Ok(value) = env::var("JARVIS_BROWSER_PROFILE_DIR") {
            config.browser.dedicated_profile_dir = PathBuf::from(value);
        }
        if let Ok(value) = env::var("JARVIS_ALLOWED_PATHS") {
            let paths = value
                .split(':')
                .filter(|segment| !segment.trim().is_empty())
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            if !paths.is_empty() {
                config.allowed_paths = paths;
            }
        }

        config
    }

    pub fn config_summary(&self) -> String {
        let planner = self
            .provider
            .planner_model_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("unconfigured");
        let worker = self
            .provider
            .worker_model_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("unconfigured");
        let fallback = self
            .provider
            .fallback_model
            .as_deref()
            .unwrap_or("disabled");
        let sms = if self.sms.account_sid.is_some() && self.sms.to_number.is_some() {
            "configured"
        } else {
            "disabled"
        };
        let stt = if self.whisper_model_path.is_some() {
            "local model configured"
        } else {
            "whisper-cli fallback"
        };

        format!(
            "planner={} worker={} fallback={} browser={} sidecar={} sms={} stt={}",
            planner,
            worker,
            fallback,
            self.primary_browser,
            self.browser.sidecar_endpoint,
            sms,
            stt
        )
    }
}

fn detect_project_root() -> Option<PathBuf> {
    for root in project_root_candidates() {
        if root.join("Cargo.toml").exists() && root.join("jarvis_rs").exists() {
            return Some(root);
        }
    }
    None
}

fn project_root_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(exe_path) = env::current_exe() {
        if let Some(root) = exe_path
            .parent()
            .and_then(|debug| debug.parent())
            .and_then(|target| target.parent())
        {
            candidates.push(root.to_path_buf());
        }
    }

    if let Ok(current_dir) = env::current_dir() {
        candidates.push(current_dir);
    }

    candidates
}

fn auto_detect_whisper_model() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    for root in project_root_candidates() {
        candidates.push(root.join(".tooling/models/ggml-tiny.en.bin"));
        candidates.push(root.join(".tooling/models/ggml-base.en.bin"));
    }

    candidates.push(PathBuf::from("/opt/homebrew/share/whisper-cpp/for-tests-ggml-tiny.bin"));
    candidates.push(PathBuf::from("/usr/local/share/whisper-cpp/for-tests-ggml-tiny.bin"));

    candidates.into_iter().find(|path| path.exists())
}

fn auto_detect_llama_model() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    for root in project_root_candidates() {
        candidates.push(root.join(".tooling/models/qwen2.5-coder-1.5b-instruct-q4_k_m.gguf"));
        candidates.push(root.join(".tooling/models/qwen2.5-1.5b-instruct-q4_k_m.gguf"));
        candidates.push(root.join(".tooling/models/qwen2.5-coder-3b-instruct-q4_k_m.gguf"));
    }

    candidates.into_iter().find(|path| path.exists())
}
