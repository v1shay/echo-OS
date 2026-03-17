use anyhow::{Context, Result};
use async_trait::async_trait;
use axum::extract::{Form, State};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use serde::Deserialize;
use std::collections::HashMap;
use tokio::net::TcpListener;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info};

use crate::config::SmsConfig;

#[derive(Debug, Clone)]
pub enum SmsApprovalAction {
    Approve,
    Reject,
}

#[async_trait]
pub trait SmsService: Send + Sync {
    async fn send_message(&self, message: &str) -> Result<()>;
    fn is_configured(&self) -> bool;
}

#[derive(Debug, Default)]
pub struct NoopSmsService;

#[async_trait]
impl SmsService for NoopSmsService {
    async fn send_message(&self, _message: &str) -> Result<()> {
        Ok(())
    }

    fn is_configured(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
pub struct TwilioSmsService {
    config: SmsConfig,
    client: reqwest::Client,
}

impl TwilioSmsService {
    pub fn new(config: SmsConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    fn endpoint(&self) -> Option<String> {
        self.config.account_sid.as_ref().map(|sid| {
            format!(
                "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
                sid
            )
        })
    }
}

#[async_trait]
impl SmsService for TwilioSmsService {
    async fn send_message(&self, message: &str) -> Result<()> {
        let endpoint = match self.endpoint() {
            Some(endpoint) => endpoint,
            None => return Ok(()),
        };

        let account_sid = match self.config.account_sid.as_ref() {
            Some(value) => value,
            None => return Ok(()),
        };
        let auth_token = match self.config.auth_token.as_ref() {
            Some(value) => value,
            None => return Ok(()),
        };
        let from_number = match self.config.from_number.as_ref() {
            Some(value) => value,
            None => return Ok(()),
        };
        let to_number = match self.config.to_number.as_ref() {
            Some(value) => value,
            None => return Ok(()),
        };

        self.client
            .post(endpoint)
            .basic_auth(account_sid, Some(auth_token))
            .form(&[
                ("From", from_number.as_str()),
                ("To", to_number.as_str()),
                ("Body", message),
            ])
            .send()
            .await
            .context("failed to send Twilio SMS")?
            .error_for_status()
            .context("Twilio SMS request returned an error")?;

        Ok(())
    }

    fn is_configured(&self) -> bool {
        self.config.account_sid.is_some()
            && self.config.auth_token.is_some()
            && self.config.from_number.is_some()
            && self.config.to_number.is_some()
    }
}

#[derive(Clone)]
struct WebhookState {
    approvals: UnboundedSender<SmsApprovalAction>,
}

#[derive(Debug, Deserialize)]
struct TwilioReplyPayload {
    #[serde(rename = "Body")]
    body: Option<String>,
}

pub async fn start_approval_webhook(
    bind_addr: String,
    approvals: UnboundedSender<SmsApprovalAction>,
) -> Result<()> {
    let state = WebhookState { approvals };
    let app = Router::new()
        .route("/twilio/reply", post(handle_twilio_reply))
        .with_state(state);
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("failed to bind SMS webhook on {}", bind_addr))?;

    tokio::spawn(async move {
        info!("SMS approval webhook listening");
        if let Err(error) = axum::serve(listener, app).await {
            error!("SMS webhook server failed: {}", error);
        }
    });

    Ok(())
}

async fn handle_twilio_reply(
    State(state): State<WebhookState>,
    Form(form): Form<HashMap<String, String>>,
) -> impl IntoResponse {
    let payload = TwilioReplyPayload {
        body: form.get("Body").cloned(),
    };
    if let Some(body) = payload.body {
        let trimmed = body.trim().to_ascii_lowercase();
        let action = if trimmed.starts_with('y') {
            SmsApprovalAction::Approve
        } else {
            SmsApprovalAction::Reject
        };
        let _ = state.approvals.send(action);
    }

    "ok"
}
