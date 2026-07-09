//! OpenAI speech-to-text via POST /v1/audio/transcriptions (plan 1.3).
//! BYOK — key comes from the macOS Keychain, never disk. 10 s timeout so a
//! dead network errors visibly instead of hanging dictation.

use super::{AudioBuffer, SessionCtx, Transcript, TranscriptionProvider};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use std::io::Cursor;
use std::time::Duration;

pub struct OpenAiStt {
    api_key: String,
    /// "gpt-4o-transcribe" | "gpt-4o-mini-transcribe" | "whisper-1" —
    /// stored as a plain string; API model names churn.
    model: String,
}

impl OpenAiStt {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }

    fn request(&self, audio: &AudioBuffer, ctx: &SessionCtx) -> Result<String> {
        let wav = encode_wav_16k_mono(&audio.samples_16k)?;

        let boundary = format!("----localflow{:x}", std::process::id() as u64 ^ 0x51ab);
        let mut body: Vec<u8> = Vec::with_capacity(wav.len() + 1024);
        let mut field = |name: &str, value: &str| {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
                    .as_bytes(),
            );
        };
        field("model", &self.model);
        field("response_format", "json");
        if let Some(lang) = &ctx.language_hint {
            if lang != "auto" {
                field("language", lang);
            }
        }
        if !ctx.initial_prompt.is_empty() {
            field("prompt", &ctx.initial_prompt);
        }
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\n\
              Content-Type: audio/wav\r\n\r\n",
        );
        body.extend_from_slice(&wav);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let mut res = ureq::post("https://api.openai.com/v1/audio/transcriptions")
            .header("Authorization", &format!("Bearer {}", self.api_key))
            .header(
                "Content-Type",
                &format!("multipart/form-data; boundary={boundary}"),
            )
            .config()
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .send(&body[..])
            .map_err(|e| match e {
                ureq::Error::StatusCode(401) => anyhow!("OpenAI: invalid API key (401)"),
                ureq::Error::StatusCode(429) => anyhow!("OpenAI: rate limited (429)"),
                ureq::Error::Timeout(_) => anyhow!("OpenAI: request timed out after 10 s"),
                other => anyhow!("OpenAI request failed: {other}"),
            })?;

        let body_text = res
            .body_mut()
            .read_to_string()
            .context("reading OpenAI response")?;
        let json: serde_json::Value =
            serde_json::from_str(&body_text).context("parsing OpenAI response")?;
        let text = json["text"]
            .as_str()
            .ok_or_else(|| anyhow!("OpenAI response missing 'text': {json}"))?;
        Ok(text.trim().to_string())
    }
}

#[async_trait]
impl TranscriptionProvider for OpenAiStt {
    fn id(&self) -> &'static str {
        "openai"
    }

    fn is_local(&self) -> bool {
        false
    }

    async fn transcribe(&self, audio: &AudioBuffer, ctx: &SessionCtx) -> Result<Transcript> {
        if audio.samples_16k.is_empty() {
            bail!("empty audio buffer");
        }
        let text = self.request(audio, ctx)?;
        Ok(Transcript { text })
    }
}

/// f32 samples → 16-bit PCM WAV bytes (16 kHz mono), in memory.
fn encode_wav_16k_mono(samples: &[f32]) -> Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: crate::audio::TARGET_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
        for &s in samples {
            writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
        }
        writer.finalize()?;
    }
    Ok(cursor.into_inner())
}
