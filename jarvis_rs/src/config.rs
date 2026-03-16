use std::env;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub fallback_base_url: Option<String>,
    pub fallback_model: Option<String>,
    pub fallback_api_key: Option<String>,
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
}

impl Default for AppConfig {
    fn default() -> Self {
        let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let home_dir = env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| current_dir.clone());

        let mut allowed_paths = vec![current_dir.clone()];
        allowed_paths.push(home_dir.join("Desktop"));
        allowed_paths.push(home_dir.join("Documents"));

        Self {
            provider: ProviderConfig {
                ollama_base_url: "http://127.0.0.1:11434".to_string(),
                ollama_model: "llama3".to_string(),
                fallback_base_url: None,
                fallback_model: None,
                fallback_api_key: None,
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
        }
    }
}

impl AppConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(value) = env::var("JARVIS_OLLAMA_BASE_URL") {
            config.provider.ollama_base_url = value;
        }
        if let Ok(value) = env::var("JARVIS_OLLAMA_MODEL") {
            config.provider.ollama_model = value;
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
            "ollama={} fallback={} sms={} stt={} webhook={}",
            self.provider.ollama_model, fallback, sms, stt, self.sms.webhook_bind
        )
    }
}
