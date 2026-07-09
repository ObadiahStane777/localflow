//! Local LLM formatting via the Ollama HTTP API (plan 2.2).
//! Recommended model: Qwen 2.5 3B class — ~300-600 ms for a 40-word dictation
//! on Apple Silicon. keep_alive on every request keeps the model warm; the
//! pipeline additionally pings every 4 minutes while this formatter is active.

use super::{FormatCtx, FormattingProvider};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;

pub struct OllamaFormatter {
    base_url: String,
    model: String,
}

impl OllamaFormatter {
    pub fn new(base_url: String, model: String) -> Self {
        Self { base_url, model }
    }

    pub fn server_up(&self) -> bool {
        ureq::get(format!("{}/api/tags", self.base_url))
            .config()
            .timeout_global(Some(Duration::from_millis(800)))
            .build()
            .call()
            .is_ok()
    }

    pub fn model_available(&self) -> bool {
        let Ok(mut res) = ureq::get(format!("{}/api/tags", self.base_url))
            .config()
            .timeout_global(Some(Duration::from_millis(800)))
            .build()
            .call()
        else {
            return false;
        };
        let Ok(body) = res.body_mut().read_to_string() else {
            return false;
        };
        // Match with or without tag: "qwen2.5:3b" or "qwen2.5"
        let base = self.model.split(':').next().unwrap_or(&self.model);
        body.contains(&format!("\"name\":\"{}\"", self.model)) || body.contains(base)
    }

    /// Refreshes the server-side keep_alive TTL so the first dictation after a
    /// pause doesn't pay the model-load cost.
    pub fn keep_alive_ping(base_url: &str, model: &str) {
        let _ = ureq::post(format!("{base_url}/api/generate"))
            .config()
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .send_json(json!({ "model": model, "keep_alive": "10m" }));
    }
}

#[async_trait]
impl FormattingProvider for OllamaFormatter {
    fn id(&self) -> &'static str {
        "ollama"
    }

    fn is_local(&self) -> bool {
        true
    }

    async fn format(&self, raw: &str, ctx: &FormatCtx) -> Result<String> {
        let payload = json!({
            "model": self.model,
            "stream": false,
            "keep_alive": "10m",
            "options": { "temperature": 0 },
            "messages": [
                { "role": "system", "content": crate::prompt::system_prompt(ctx) },
                { "role": "user", "content": crate::prompt::user_payload(raw, ctx) }
            ]
        });
        let mut res = ureq::post(format!("{}/api/chat", self.base_url))
            .config()
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .send_json(&payload)
            .map_err(|e| anyhow!("Ollama request failed: {e}"))?;
        let body = res
            .body_mut()
            .read_to_string()
            .context("reading Ollama response")?;
        let json: serde_json::Value =
            serde_json::from_str(&body).context("parsing Ollama response")?;
        let text = json["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("Ollama response missing message.content: {body}"))?;
        Ok(strip_llm_wrapping(text))
    }
}

/// Small models sometimes wrap output in quotes or a "Cleaned text:" prefix
/// despite instructions — strip the obvious cases, and drop any <think> block
/// from reasoning-tuned models.
pub fn strip_llm_wrapping(text: &str) -> String {
    let mut t = text.trim();
    if let Some(end) = t.find("</think>") {
        t = t[end + "</think>".len()..].trim();
    }
    for prefix in ["Cleaned text:", "Cleaned:", "Output:", "Text:"] {
        if let Some(rest) = t.strip_prefix(prefix) {
            t = rest.trim();
        }
    }
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t = &t[1..t.len() - 1];
    }
    t.trim().to_string()
}
