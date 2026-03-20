use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;

/// Google Calendar integration for Jarvis
#[derive(Debug)]
pub struct CalendarService {
    api_key: Option<String>,
    oauth_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub summary: String,
    pub description: Option<String>,
    pub start: EventDateTime,
    pub end: EventDateTime,
    pub location: Option<String>,
    pub attendees: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDateTime {
    pub datetime: String,
    pub timezone: Option<String>,
}

impl CalendarService {
    pub fn new(api_key: Option<String>, oauth_token: Option<String>) -> Self {
        Self {
            api_key,
            oauth_token,
        }
    }

    /// Get events for today
    pub async fn get_todays_events(&self) -> Result<Vec<CalendarEvent>> {
        self.get_events_for_date(&chrono::Local::now().format("%Y-%m-%d").to_string())
            .await
    }

    /// Get events for a specific date (YYYY-MM-DD format)
    pub async fn get_events_for_date(&self, date: &str) -> Result<Vec<CalendarEvent>> {
        let token = self
            .oauth_token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Google Calendar OAuth token not configured"))?;

        let time_min = format!("{}T00:00:00Z", date);
        let time_max = format!("{}T23:59:59Z", date);

        let url = format!(
            "https://www.googleapis.com/calendar/v3/calendars/primary/events?timeMin={}&timeMax={}&singleEvents=true&orderBy=startTime",
            urlencoding::encode(&time_min),
            urlencoding::encode(&time_max)
        );

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?;

        let json: serde_json::Value = response.json().await?;
        let empty = Vec::new();
        let items = json["items"].as_array().unwrap_or(&empty);

        let events = items
            .iter()
            .filter_map(|item| self.parse_event(item).ok())
            .collect();

        Ok(events)
    }

    /// Get upcoming events (next N hours)
    pub async fn get_upcoming_events(&self, hours: u32) -> Result<Vec<CalendarEvent>> {
        let token = self
            .oauth_token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Google Calendar OAuth token not configured"))?;

        let now = chrono::Utc::now();
        let later = now + chrono::Duration::hours(hours as i64);

        let time_min = now.to_rfc3339();
        let time_max = later.to_rfc3339();

        let url = format!(
            "https://www.googleapis.com/calendar/v3/calendars/primary/events?timeMin={}&timeMax={}&singleEvents=true&orderBy=startTime",
            urlencoding::encode(&time_min),
            urlencoding::encode(&time_max)
        );

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?;

        let json: serde_json::Value = response.json().await?;
        let empty = Vec::new();
        let items = json["items"].as_array().unwrap_or(&empty);

        let events = items
            .iter()
            .filter_map(|item| self.parse_event(item).ok())
            .collect();

        Ok(events)
    }

    /// Find events matching a search query
    pub async fn search_events(&self, query: &str) -> Result<Vec<CalendarEvent>> {
        let token = self
            .oauth_token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Google Calendar OAuth token not configured"))?;

        let url = format!(
            "https://www.googleapis.com/calendar/v3/calendars/primary/events?q={}",
            urlencoding::encode(query)
        );

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?;

        let json: serde_json::Value = response.json().await?;
        let empty = Vec::new();
        let items = json["items"].as_array().unwrap_or(&empty);

        let events = items
            .iter()
            .filter_map(|item| self.parse_event(item).ok())
            .collect();

        Ok(events)
    }

    /// Get next event
    pub async fn get_next_event(&self) -> Result<Option<CalendarEvent>> {
        let mut upcoming = self.get_upcoming_events(24).await?;
        let next = upcoming.into_iter().next();
        Ok(next)
    }

    /// Format events as readable text
    pub fn format_events(&self, events: &[CalendarEvent]) -> String {
        if events.is_empty() {
            return "No events found.".to_string();
        }

        let mut result = String::new();
        for event in events {
            result.push_str(&format!(
                "• {} at {}\n",
                event.summary,
                self.format_time(&event.start.datetime)
            ));
            if let Some(location) = &event.location {
                result.push_str(&format!("  Location: {}\n", location));
            }
            if let Some(description) = &event.description {
                if !description.is_empty() {
                    result.push_str(&format!("  {}\n", description));
                }
            }
        }
        result
    }

    fn format_time(&self, datetime_str: &str) -> String {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(datetime_str) {
            dt.format("%I:%M %p").to_string()
        } else {
            datetime_str.to_string()
        }
    }

    fn parse_event(&self, item: &serde_json::Value) -> Result<CalendarEvent> {
        Ok(CalendarEvent {
            id: item["id"].as_str().unwrap_or("").to_string(),
            summary: item["summary"].as_str().unwrap_or("(No title)").to_string(),
            description: item["description"].as_str().map(String::from),
            start: EventDateTime {
                datetime: item["start"]["dateTime"]
                    .as_str()
                    .or(item["start"]["date"].as_str())
                    .unwrap_or("")
                    .to_string(),
                timezone: item["start"]["timeZone"].as_str().map(String::from),
            },
            end: EventDateTime {
                datetime: item["end"]["dateTime"]
                    .as_str()
                    .or(item["end"]["date"].as_str())
                    .unwrap_or("")
                    .to_string(),
                timezone: item["end"]["timeZone"].as_str().map(String::from),
            },
            location: item["location"].as_str().map(String::from),
            attendees: item["attendees"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|a| a["email"].as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            status: item["status"].as_str().unwrap_or("confirmed").to_string(),
        })
    }

    /// Check if service is configured
    pub fn is_configured(&self) -> bool {
        self.oauth_token.is_some()
    }
}

/// Helper to get OAuth token from macOS Keychain (if stored)
pub fn get_calendar_token_from_keychain() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("security")
            .args(&[
                "find-generic-password",
                "-s",
                "jarvis-google-calendar",
                "-w",
            ])
            .output()
            .ok()?;

        if output.status.success() {
            let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !token.is_empty() {
                return Some(token);
            }
        }
    }
    None
}

/// Helper to store OAuth token in macOS Keychain
#[cfg(target_os = "macos")]
pub fn store_calendar_token_in_keychain(token: &str) -> Result<()> {
    Command::new("security")
        .args(&[
            "add-generic-password",
            "-a",
            "jarvis",
            "-s",
            "jarvis-google-calendar",
            "-w",
            token,
            "-U",
        ])
        .output()
        .context("Failed to store token in keychain")?;
    Ok(())
}
