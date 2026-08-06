//! Provider layer (plan 1.1). Everything pluggable lives behind
//! `TranscriptionProvider` (and `FormattingProvider`, Phase 2); the pipeline
//! never knows which is active.

pub mod ollama_fmt;
pub mod openai_fmt;
pub mod openai_stt;
pub mod parakeet;
pub mod passthrough;
pub mod whisper_cpp;

use anyhow::{bail, Result};
use async_trait::async_trait;

pub struct AudioBuffer {
    /// 16 kHz mono f32, VAD-trimmed.
    pub samples_16k: Vec<f32>,
}

pub struct SessionCtx {
    pub language_hint: Option<String>,
    /// Dictionary bias — fed to Whisper's initial_prompt / API prompt field.
    pub initial_prompt: String,
}

pub struct Transcript {
    pub text: String,
}

#[async_trait]
pub trait TranscriptionProvider: Send {
    fn id(&self) -> &'static str; // "whisper-cpp" | "parakeet" | "openai"
    fn is_local(&self) -> bool;
    async fn transcribe(&self, audio: &AudioBuffer, ctx: &SessionCtx) -> Result<Transcript>;
}

pub const STT_PROVIDER_IDS: &[&str] = &["whisper-cpp", "parakeet", "openai"];

// ---- Formatting (plan 2.1) ----

#[derive(Clone, Debug)]
pub enum Mode {
    Dictate,
    Command { selection: String },
}

pub struct FormatCtx {
    pub app_id: String,
    pub style: crate::store::StyleProfile,
    pub dictionary: Vec<crate::store::DictEntry>,
    pub snippets: Vec<crate::store::Snippet>,
    pub mode: Mode,
}

#[async_trait]
pub trait FormattingProvider: Send {
    fn id(&self) -> &'static str; // "ollama" | "openai" | "passthrough"
    fn is_local(&self) -> bool;
    async fn format(&self, raw: &str, ctx: &FormatCtx) -> Result<String>;
}

/// Detects when a small local model broke character and replied conversationally
/// ABOUT the cleaning task instead of returning the cleaned transcript — the
/// classic "I understand you need… Here is the cleaned text based on your
/// dictation:" failure. These are meta-references to the task itself, which
/// essentially never appear in genuine dictation, so a hit is a safe signal to
/// discard the output and fall back to the passthrough cleaner on the raw text.
/// ponytail: tell-list, not a classifier — extend the list if a new opener leaks.
pub fn looks_like_meta_reply(text: &str) -> bool {
    let low = text.trim().to_lowercase();
    const TELLS: &[&str] = &[
        "here is the cleaned",
        "here's the cleaned",
        "here is the polished",
        "here's the polished",
        "here is the transcript",
        "here's the transcript",
        "cleaned text based on",
        "cleaned version of",
        "polished version of",
        "based on your dictation",
        "i understand you need",
        "i understand you want",
        "i understand that you",
        "sure, here is",
        "sure, here's",
        "as requested, here",
        "i've set",
        "i have set",
        "meeting set for",
        "sure thing",
    ];
    TELLS.iter().any(|t| low.contains(t))
}

/// Constructs the active formatter from settings. "auto" = Ollama when its
/// server responds, else passthrough (graceful degradation, plan 2.2).
pub fn build_fmt_provider(
    id: &str,
    store: &crate::store::Store,
) -> Result<Box<dyn FormattingProvider>> {
    match id {
        "passthrough" => Ok(Box::new(passthrough::Passthrough)),
        "ollama" => {
            let url = store.setting_or("ollama_url", "http://localhost:11434");
            let model = store.setting_or("ollama_model", "qwen2.5:3b");
            let ollama = ollama_fmt::OllamaFormatter::new(url, model);
            if !ollama.server_up() {
                bail!("Ollama server is not responding — start Ollama or pick another formatter");
            }
            Ok(Box::new(ollama))
        }
        "openai" => {
            let model = store.setting_or("openai_fmt_model", "gpt-4o-mini");
            let key = crate::commands::read_api_key()?
                .ok_or_else(|| anyhow::anyhow!("no OpenAI API key set — add one in Settings"))?;
            Ok(Box::new(openai_fmt::OpenAiFormatter::new(key, model)))
        }
        "auto" => {
            let url = store.setting_or("ollama_url", "http://localhost:11434");
            let model = store.setting_or("ollama_model", "qwen2.5:3b");
            let ollama = ollama_fmt::OllamaFormatter::new(url, model);
            if ollama.server_up() && ollama.model_available() {
                Ok(Box::new(ollama))
            } else {
                tracing::info!("auto formatter: Ollama unavailable, using passthrough");
                Ok(Box::new(passthrough::Passthrough))
            }
        }
        other => bail!("unknown formatting provider '{other}'"),
    }
}

/// Constructs the active provider from settings + installed models.
/// Local providers load their model here (expensive) — the pipeline caches
/// the box and only rebuilds on a ReloadProvider control message.
pub fn build_stt_provider(
    id: &str,
    models_dir: &std::path::Path,
    store: &crate::store::Store,
) -> Result<Box<dyn TranscriptionProvider>> {
    match id {
        "whisper-cpp" => {
            let model_id = store.setting_or("whisper_model", "whisper-small");
            let path = crate::models::whisper_model_path(models_dir, &model_id)?;
            if !path.exists() {
                bail!("model {model_id} is not downloaded — open Models and download it");
            }
            Ok(Box::new(whisper_cpp::WhisperCpp::load(&path)?))
        }
        "parakeet" => {
            let dir = models_dir.join(crate::models::PARAKEET_DIR);
            if !dir.join("vocab.txt").exists() {
                bail!("Parakeet model is not downloaded — open Models and download it");
            }
            Ok(Box::new(parakeet::Parakeet::load(&dir)?))
        }
        "openai" => {
            let model = store.setting_or("openai_stt_model", "gpt-4o-mini-transcribe");
            let key = crate::commands::read_api_key()?
                .ok_or_else(|| anyhow::anyhow!("no OpenAI API key set — add one in Settings"))?;
            Ok(Box::new(openai_stt::OpenAiStt::new(key, model)))
        }
        other => bail!("unknown transcription provider '{other}'"),
    }
}
