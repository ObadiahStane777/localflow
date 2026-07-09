//! whisper.cpp transcription via whisper-rs, Metal-accelerated (plan 0.5/1.1).

use super::{AudioBuffer, SessionCtx, Transcript, TranscriptionProvider};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct WhisperCpp {
    ctx: WhisperContext,
}

impl WhisperCpp {
    /// Loads a GGUF/GGML model. Expensive (~0.5-2 s) — do it once at startup
    /// or provider switch, never per-utterance.
    pub fn load(model_path: &Path) -> Result<Self> {
        let ctx = WhisperContext::new_with_params(
            model_path
                .to_str()
                .context("model path is not valid UTF-8")?,
            WhisperContextParameters::default(),
        )
        .context("loading whisper model")?;
        Ok(Self { ctx })
    }

    fn run(&self, samples_16k: &[f32], ctx: &SessionCtx) -> Result<String> {
        // whisper.cpp rejects inputs under ~1 s — pad short utterances with
        // trailing silence.
        let min_len = (crate::audio::TARGET_RATE as usize * 11) / 10;
        let mut padded;
        let input: &[f32] = if samples_16k.len() < min_len {
            padded = samples_16k.to_vec();
            padded.resize(min_len, 0.0);
            &padded
        } else {
            samples_16k
        };

        let mut state = self.ctx.create_state().context("creating whisper state")?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let lang = ctx.language_hint.as_deref().unwrap_or("auto");
        params.set_language(Some(lang));
        if !ctx.initial_prompt.is_empty() {
            params.set_initial_prompt(&ctx.initial_prompt);
        }
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_no_context(true);

        state.full(params, input).context("whisper inference")?;

        let n = state.full_n_segments();
        let mut text = String::new();
        for i in 0..n {
            if let Some(segment) = state.get_segment(i) {
                text.push_str(&segment.to_str_lossy().context("reading segment text")?);
            }
        }
        Ok(text.trim().to_string())
    }
}

#[async_trait]
impl TranscriptionProvider for WhisperCpp {
    fn id(&self) -> &'static str {
        "whisper-cpp"
    }

    fn is_local(&self) -> bool {
        true
    }

    async fn transcribe(&self, audio: &AudioBuffer, ctx: &SessionCtx) -> Result<Transcript> {
        let text = self.run(&audio.samples_16k, ctx)?;
        Ok(Transcript { text })
    }
}
