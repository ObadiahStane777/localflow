//! Audio capture (plan 0.3) and VAD gating (plan 0.4).
//!
//! cpal default-input capture at the device's native rate/channels, downmixed
//! to mono in the callback, then resampled once at stop() to the 16 kHz f32
//! that Whisper and Silero both expect.
//!
//! NOTE: cpal::Stream is !Send — a Recorder must be created and dropped on the
//! same thread (the pipeline thread owns it).

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::sync::{Arc, Mutex};
use voice_activity_detector::VoiceActivityDetector;

pub const TARGET_RATE: u32 = 16_000;

pub struct Recorder {
    // Kept alive for the duration of the recording; dropping stops capture.
    _stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    source_rate: u32,
}

/// Names of available input devices (plan 1.8 device picker).
pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devs| {
            devs.filter_map(|d| d.description().ok().map(|desc| desc.name().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

impl Recorder {
    /// Opens the requested (or default) input device and starts capturing
    /// immediately.
    pub fn start(device_name: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();
        let device = match device_name {
            Some(wanted) if !wanted.is_empty() && wanted != "default" => host
                .input_devices()?
                .find(|d| {
                    d.description()
                        .map(|desc| desc.name() == wanted)
                        .unwrap_or(false)
                })
                .ok_or_else(|| anyhow!("input device '{wanted}' not found"))?,
            _ => host
                .default_input_device()
                .ok_or_else(|| anyhow!("no default input device — is a microphone connected?"))?,
        };
        let config = device
            .default_input_config()
            .context("querying default input config")?;
        let source_rate = config.sample_rate();
        let channels = config.channels() as usize;
        let buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(
            source_rate as usize * 30,
        )));

        let err_fn = |e| tracing::error!("audio stream error: {e}");
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let buf = buf.clone();
                device.build_input_stream(
                    config.into(),
                    move |data: &[f32], _| push_mono(&buf, data, channels),
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::I16 => {
                let buf = buf.clone();
                device.build_input_stream(
                    config.into(),
                    move |data: &[i16], _| {
                        let f: Vec<f32> =
                            data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                        push_mono(&buf, &f, channels)
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::U16 => {
                let buf = buf.clone();
                device.build_input_stream(
                    config.into(),
                    move |data: &[u16], _| {
                        let f: Vec<f32> = data
                            .iter()
                            .map(|&s| (s as f32 - 32_768.0) / 32_768.0)
                            .collect();
                        push_mono(&buf, &f, channels)
                    },
                    err_fn,
                    None,
                )?
            }
            other => return Err(anyhow!("unsupported input sample format {other:?}")),
        };
        stream.play().context("starting input stream")?;
        tracing::debug!("recording started ({source_rate} Hz, {channels} ch)");
        Ok(Self {
            _stream: stream,
            buf,
            source_rate,
        })
    }

    /// Stops capture and returns the whole utterance as 16 kHz mono f32.
    pub fn stop(self) -> Result<Vec<f32>> {
        let source_rate = self.source_rate;
        let buf = self.buf.clone();
        drop(self._stream);
        let samples = std::mem::take(&mut *buf.lock().expect("audio buf poisoned"));
        tracing::debug!(
            "recording stopped: {} samples ({:.2}s)",
            samples.len(),
            samples.len() as f32 / source_rate as f32
        );
        resample_to_16k(&samples, source_rate)
    }
}

fn push_mono(buf: &Arc<Mutex<Vec<f32>>>, interleaved: &[f32], channels: usize) {
    let mut guard = buf.lock().expect("audio buf poisoned");
    if channels <= 1 {
        guard.extend_from_slice(interleaved);
    } else {
        guard.extend(
            interleaved
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32),
        );
    }
}

/// Whole-buffer resample via rubato SincFixedIn, processing in fixed chunks.
pub fn resample_to_16k(samples: &[f32], source_rate: u32) -> Result<Vec<f32>> {
    if source_rate == TARGET_RATE || samples.is_empty() {
        return Ok(samples.to_vec());
    }
    let params = SincInterpolationParameters {
        sinc_len: 128,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };
    const CHUNK: usize = 1024;
    let ratio = TARGET_RATE as f64 / source_rate as f64;
    let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, CHUNK, 1)
        .context("building resampler")?;
    let mut out = Vec::with_capacity((samples.len() as f64 * ratio) as usize + CHUNK);

    let mut chunks = samples.chunks_exact(CHUNK);
    for chunk in &mut chunks {
        let res = resampler.process(&[chunk], None)?;
        out.extend_from_slice(&res[0]);
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        let res = resampler.process_partial(Some(&[rem]), None)?;
        out.extend_from_slice(&res[0]);
    }
    // Flush the resampler's internal delay line.
    let res = resampler.process_partial::<&[f32]>(None, None)?;
    out.extend_from_slice(&res[0]);
    Ok(out)
}

/// Silero VAD gate (plan 0.4): trims leading/trailing silence and returns
/// None when the utterance holds under 300 ms of speech — the Whisper
/// hallucination guard.
pub fn vad_gate(samples_16k: &[f32]) -> Result<Option<Vec<f32>>> {
    const CHUNK: usize = 512; // 32 ms @ 16 kHz — Silero's native chunk
    const SPEECH_PROB: f32 = 0.5;
    const MIN_SPEECH_MS: usize = 300;
    const PAD_CHUNKS: usize = 4; // ~128 ms of context kept around speech

    if samples_16k.len() < CHUNK {
        return Ok(None);
    }
    let mut vad = VoiceActivityDetector::builder()
        .sample_rate(TARGET_RATE as i64)
        .chunk_size(CHUNK)
        .build()
        .map_err(|e| anyhow!("building Silero VAD: {e}"))?;

    let chunks: Vec<&[f32]> = samples_16k.chunks(CHUNK).collect();
    let mut speech_flags = Vec::with_capacity(chunks.len());
    for chunk in &chunks {
        let prob = vad.predict(chunk.iter().copied());
        speech_flags.push(prob >= SPEECH_PROB);
    }

    let speech_chunks = speech_flags.iter().filter(|&&s| s).count();
    let speech_ms = speech_chunks * CHUNK * 1000 / TARGET_RATE as usize;
    if speech_ms < MIN_SPEECH_MS {
        tracing::info!("VAD: only {speech_ms} ms of speech — discarding utterance");
        return Ok(None);
    }

    let first = speech_flags.iter().position(|&s| s).unwrap_or(0);
    let last = speech_flags.iter().rposition(|&s| s).unwrap_or(0);
    let start = first.saturating_sub(PAD_CHUNKS) * CHUNK;
    let end = ((last + 1 + PAD_CHUNKS) * CHUNK).min(samples_16k.len());
    tracing::debug!(
        "VAD: {speech_ms} ms speech, trimmed {} -> {} samples",
        samples_16k.len(),
        end - start
    );
    Ok(Some(samples_16k[start..end].to_vec()))
}
