//! Direct DeepSeek transport over the OpenAI-compatible chat-completions API.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};

use crate::codex::ProviderEvent;
use crate::domain::{Chat, Role};
use crate::ollama::{ContextUsage, Stats};

pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
pub const DEFAULT_MODEL: &str = "deepseek-chat";
pub const MODEL_LIMIT: u32 = 65_536;
const MAX_CONTEXT_BYTES: usize = 96 * 1024;
const MAX_RECORD_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: DEFAULT_BASE_URL.into(),
            model: DEFAULT_MODEL.into(),
        }
    }
}

impl Settings {
    /// The key from settings, falling back to `DEEPSEEK_API_KEY`.
    pub fn api_key(&self) -> Option<String> {
        let saved = self.api_key.trim();
        if !saved.is_empty() {
            return Some(saved.to_owned());
        }
        std::env::var("DEEPSEEK_API_KEY")
            .ok()
            .map(|key| key.trim().to_owned())
            .filter(|key| !key.is_empty())
    }

    pub fn endpoint(&self) -> Result<url::Url, String> {
        let base = self.base_url.trim();
        let base = if base.is_empty() { DEFAULT_BASE_URL } else { base };
        let mut url = url::Url::parse(base).map_err(|error| format!("Invalid DeepSeek URL: {error}"))?;
        if !url.path().ends_with('/') {
            let path = format!("{}/", url.path());
            url.set_path(&path);
        }
        Ok(url)
    }

    pub fn model(&self) -> &str {
        let model = self.model.trim();
        if model.is_empty() { DEFAULT_MODEL } else { model }
    }
}

pub struct Request {
    pub settings: Settings,
    pub model: String,
    pub system: String,
    pub messages: Vec<Value>,
    pub desktop: crate::desktop::Settings,
    pub desktop_activity: crate::desktop::Activity,
}

impl Request {
    pub fn from_chat(chat: &Chat, settings: Settings) -> Self {
        Self {
            model: settings.model().to_owned(),
            messages: history(&chat.messages),
            system: system_prompt(chat),
            settings,
            desktop: Default::default(),
            desktop_activity: Default::default(),
        }
    }
}

fn history(messages: &[crate::domain::Message]) -> Vec<Value> {
    let mut rendered = Vec::new();
    let mut used = 0usize;
    for message in messages.iter().rev() {
        let content = message.content.trim();
        if content.is_empty() || message.error.is_some() {
            continue;
        }
        used = used.saturating_add(content.len());
        if used > MAX_CONTEXT_BYTES && !rendered.is_empty() {
            break;
        }
        rendered.push(json!({
            "role": match message.role { Role::User => "user", Role::Assistant => "assistant" },
            "content": content,
        }));
    }
    rendered.reverse();
    rendered
}

fn system_prompt(chat: &Chat) -> String {
    let mut prompt = String::from(
        "You are ReDox, a native Rust desktop assistant. Answer directly and keep replies short unless the user asks for detail. Use Markdown for code and lists.",
    );
    if let Some(workspace) = &chat.workspace {
        prompt.push_str(&format!(
            "\n\nThe open workspace is {}. You cannot read or change files in this build; describe changes for the user instead.",
            workspace.display()
        ));
    }
    prompt.push_str(&format!("\n\nAccess mode: {}.", chat.mode.label()));
    prompt
}

pub async fn stream_chat(
    request: Request,
    sender: mpsc::Sender<ProviderEvent>,
    mut cancel: watch::Receiver<bool>,
) -> Result<(), String> {
    if *cancel.borrow() {
        return Err("Cancelled.".into());
    }
    tokio::select! {
        result = run(request, sender) => result,
        _ = cancel.changed() => Err("Cancelled.".into()),
    }
}

async fn run(request: Request, sender: mpsc::Sender<ProviderEvent>) -> Result<(), String> {
    let key = request
        .settings
        .api_key()
        .ok_or("Add a DeepSeek API key in Settings > Models, or set DEEPSEEK_API_KEY.")?;
    let url = request
        .settings
        .endpoint()?
        .join("chat/completions")
        .map_err(|error| format!("Invalid DeepSeek endpoint: {error}"))?;

    let mut messages = Vec::with_capacity(request.messages.len() + 1);
    messages.push(json!({"role": "system", "content": request.system}));
    messages.extend(request.messages);
    let body = json!({
        "model": request.model,
        "messages": messages,
        "stream": true,
        "stream_options": {"include_usage": true},
    });

    send(&sender, ProviderEvent::Status("Contacting DeepSeek…".into())).await?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|error| format!("DeepSeek client error: {error}"))?;
    let response = client
        .post(url)
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("DeepSeek request failed: {error}"))?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!(
            "DeepSeek request failed ({}): {}",
            status.as_u16(),
            error_message(&text)
        ));
    }

    let started = Instant::now();
    let mut buffer: Vec<u8> = Vec::new();
    let mut reply = String::new();
    let mut usage: Option<(u64, u64, u64, Option<u64>)> = None;
    let mut done = false;
    let mut response = response;

    while !done {
        let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| format!("DeepSeek stream failed: {error}"))?
        else {
            break;
        };
        for part in chunk.split_inclusive(|byte| *byte == b'\n') {
            let record = part.strip_suffix(b"\n");
            let bytes = record.unwrap_or(part);
            if bytes.len() > MAX_RECORD_BYTES - buffer.len() {
                return Err("DeepSeek stream record exceeded the 2 MiB limit.".into());
            }
            if record.is_none() {
                buffer.extend_from_slice(bytes);
                continue;
            }
            let line = if buffer.is_empty() {
                bytes
            } else {
                buffer.extend_from_slice(bytes);
                &buffer
            };
            let line = String::from_utf8_lossy(line);
            let line = line.trim();
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data == "[DONE]" {
                    done = true;
                } else if let Ok(value) = serde_json::from_str::<Value>(data) {
                    if let Some(content) = value["choices"][0]["delta"]["content"].as_str()
                        && !content.is_empty()
                    {
                        reply.push_str(content);
                        send(&sender, ProviderEvent::Content(content.to_owned())).await?;
                    }
                    if let Some(tokens) = value.get("usage").filter(|usage| !usage.is_null()) {
                        let prompt = tokens["prompt_tokens"].as_u64().unwrap_or(0);
                        let completion = tokens["completion_tokens"].as_u64().unwrap_or(0);
                        let total = tokens["total_tokens"].as_u64().unwrap_or(prompt + completion);
                        let cached = tokens["prompt_cache_hit_tokens"].as_u64();
                        usage = Some((prompt, completion, total, cached));
                    }
                    if let Some(error) = value.get("error") {
                        return Err(format!("DeepSeek error: {}", error_message(&error.to_string())));
                    }
                }
            }
            buffer.clear();
            if done {
                break;
            }
        }
    }

    if reply.trim().is_empty() {
        return Err("DeepSeek returned an empty reply.".into());
    }

    let (prompt, completion, total, cached) = usage.unwrap_or_else(|| {
        (estimated_tokens(&reply), 0, estimated_tokens(&reply), None)
    });
    let stats = Stats {
        prompt_tokens: prompt,
        output_tokens: completion,
        eval_seconds: started.elapsed().as_secs_f64(),
        requests: 1,
    };
    send(&sender, ProviderEvent::Metrics(stats)).await?;
    send(
        &sender,
        ProviderEvent::Context(ContextUsage {
            tokens: total.max(prompt.saturating_add(completion)),
            limit: MODEL_LIMIT,
            compressions: 0,
            estimated: usage.is_none(),
            load_seconds: None,
            prompt_seconds: None,
            cached_tokens: cached,
        }),
    )
    .await?;
    send(&sender, ProviderEvent::Status("DeepSeek reply ready.".into())).await?;
    Ok(())
}

/// Rough token estimate for providers that do not report usage.
fn estimated_tokens(text: &str) -> u64 {
    (text.chars().count() as u64).div_ceil(4).max(1)
}

fn error_message(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body)
        && let Some(message) = value["error"]["message"].as_str()
    {
        return message.to_owned();
    }
    let text = body.trim();
    if text.is_empty() {
        "no response body".into()
    } else {
        text.chars().take(300).collect()
    }
}

async fn send(sender: &mpsc::Sender<ProviderEvent>, event: ProviderEvent) -> Result<(), String> {
    sender.send(event).await.map_err(|_| "Chat closed.".to_owned())
}

#[cfg(test)]
#[path = "../tests/transport/deepseek.rs"]
mod tests;
