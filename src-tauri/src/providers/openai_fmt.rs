//! GPT formatting via OpenAI Chat Completions (plan 2.3). Highest quality for
//! Command Mode rewrites and tone work. Temperature 0, 8 s timeout — on any
//! failure the pipeline falls back to passthrough with the raw kept.

use super::{FormatCtx, FormattingProvider};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;

pub struct OpenAiFormatter {
    api_key: String,
    /// Plain string — API model names churn (plan 2.3).
    model: String,
}

impl OpenAiFormatter {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }
}

#[async_trait]
impl FormattingProvider for OpenAiFormatter {
    fn id(&self) -> &'static str {
        "openai"
    }

    fn is_local(&self) -> bool {
        false
    }

    async fn format(&self, raw: &str, ctx: &FormatCtx) -> Result<String> {
        let payload = json!({
            "model": self.model,
            "temperature": 0,
            "messages": [
                { "role": "system", "content": crate::prompt::system_prompt(ctx) },
                { "role": "user", "content": crate::prompt::user_payload(raw, ctx) }
            ]
        });
        let mut res = ureq::post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", &format!("Bearer {}", self.api_key))
            .config()
            .timeout_global(Some(Duration::from_secs(8)))
            .build()
            .send_json(&payload)
            .map_err(|e| match e {
                ureq::Error::StatusCode(401) => anyhow!("OpenAI: invalid API key (401)"),
                ureq::Error::StatusCode(429) => anyhow!("OpenAI: rate limited (429)"),
                ureq::Error::Timeout(_) => anyhow!("OpenAI: formatting timed out after 8 s"),
                other => anyhow!("OpenAI formatting failed: {other}"),
            })?;
        let body = res
            .body_mut()
            .read_to_string()
            .context("reading OpenAI response")?;
        let json: serde_json::Value =
            serde_json::from_str(&body).context("parsing OpenAI response")?;
        let text = json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("OpenAI response missing content: {body}"))?;
        Ok(super::ollama_fmt::strip_llm_wrapping(text))
    }
}
