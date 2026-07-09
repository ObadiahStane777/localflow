//! NVIDIA Parakeet TDT 0.6B v2 via transcribe-rs / ONNX (plan 1.2).
//! Very fast on CPU/Apple Silicon; English-only (surfaced in the UI).

use super::{AudioBuffer, SessionCtx, Transcript, TranscriptionProvider};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::path::Path;
use std::sync::Mutex;
use transcribe_rs::onnx::parakeet::ParakeetModel;
use transcribe_rs::onnx::Quantization;
use transcribe_rs::{SpeechModel, TranscribeOptions};

pub struct Parakeet {
    // transcribe-rs inference takes &mut self; the trait exposes &self.
    model: Mutex<ParakeetModel>,
}

impl Parakeet {
    pub fn load(model_dir: &Path) -> Result<Self> {
        let model = ParakeetModel::load(model_dir, &Quantization::Int8)
            .map_err(|e| anyhow!("loading Parakeet: {e}"))?;
        Ok(Self {
            model: Mutex::new(model),
        })
    }
}

#[async_trait]
impl TranscriptionProvider for Parakeet {
    fn id(&self) -> &'static str {
        "parakeet"
    }

    fn is_local(&self) -> bool {
        true
    }

    async fn transcribe(&self, audio: &AudioBuffer, _ctx: &SessionCtx) -> Result<Transcript> {
        // No prompt-bias support in Parakeet; the post-STT dictionary pass
        // still applies (pipeline).
        let mut model = self.model.lock().expect("parakeet mutex poisoned");
        let result = model
            .transcribe(&audio.samples_16k, &TranscribeOptions::default())
            .map_err(|e| anyhow!("parakeet inference: {e}"))?;
        Ok(Transcript {
            text: result.text.trim().to_string(),
        })
    }
}
