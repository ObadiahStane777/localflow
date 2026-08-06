//! Pipeline orchestration: hotkey loop → record → VAD → TranscriptionProvider
//! → FormattingProvider (style/dictionary/snippet aware) → inject, all on one
//! dedicated thread. Provider switching happens via ReloadProvider control
//! messages — no restart.

use crate::audio::{self, Recorder};
use crate::commands::{update_status, TranscriptEvent};
use crate::context;
use crate::hotkey::{self, HotkeyEvent};
use crate::providers::{
    self, AudioBuffer, FormatCtx, FormattingProvider, Mode, SessionCtx, TranscriptionProvider,
};
use crate::store::{self, Store};
use crate::{inject, models};
use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use serde_json::json;
use std::path::Path;
use std::sync::atomic::AtomicI64;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineMsg {
    Hotkey(HotkeyEvent),
    /// Settings changed: rebuild the active providers / reread settings.
    ReloadProvider,
}

pub struct PipelineHandle {
    pub tx: Sender<PipelineMsg>,
    pub hotkey_keycode: Arc<AtomicI64>,
}

pub fn spawn(app: AppHandle, store: Arc<Store>) -> PipelineHandle {
    let (tx, rx) = crossbeam_channel::unbounded::<PipelineMsg>();

    let keycode = store
        .setting_or("hotkey_keycode", "54")
        .parse::<i64>()
        .unwrap_or(54);
    let hotkey_keycode = Arc::new(AtomicI64::new(keycode));

    // Hotkey events funnel into the same channel as control messages.
    let hk_tx = tx.clone();
    let (raw_tx, raw_rx) = crossbeam_channel::unbounded::<HotkeyEvent>();
    hotkey::spawn(raw_tx, hotkey_keycode.clone());
    std::thread::Builder::new()
        .name("hotkey-forward".into())
        .spawn(move || {
            for ev in raw_rx {
                let _ = hk_tx.send(PipelineMsg::Hotkey(ev));
            }
        })
        .expect("failed to spawn hotkey forwarder");

    // Ollama keep-alive (plan 2.2): refresh the model TTL while an Ollama
    // formatter is the active choice.
    {
        let store = store.clone();
        std::thread::Builder::new()
            .name("ollama-keepalive".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_secs(240));
                let fmt = store.setting_or("fmt_provider", "auto");
                if fmt == "ollama" || fmt == "auto" {
                    let url = store.setting_or("ollama_url", "http://localhost:11434");
                    let model = store.setting_or("ollama_model", "qwen2.5:3b");
                    providers::ollama_fmt::OllamaFormatter::keep_alive_ping(&url, &model);
                }
            })
            .expect("failed to spawn keep-alive thread");
    }

    std::thread::Builder::new()
        .name("pipeline".into())
        .spawn(move || run(&app, &store, rx))
        .expect("failed to spawn pipeline thread");

    PipelineHandle { tx, hotkey_keycode }
}

struct Providers {
    stt: Option<Box<dyn TranscriptionProvider>>,
    fmt: Option<Box<dyn FormattingProvider>>,
}

fn load_providers(app: &AppHandle, store: &Store) -> Providers {
    let stt = (|| -> Result<Box<dyn TranscriptionProvider>> {
        let models_dir = models::models_dir(app)?;
        let id = store.setting_or("stt_provider", "whisper-cpp");
        update_status(app, |s| {
            s.model_state = "loading".into();
            s.stt_provider = id.clone();
        });
        providers::build_stt_provider(&id, &models_dir, store)
    })();
    let fmt = providers::build_fmt_provider(&store.setting_or("fmt_provider", "auto"), store);

    match (&stt, &fmt) {
        (Ok(s), Ok(f)) => {
            tracing::info!("providers ready: stt={}, fmt={}", s.id(), f.id());
            update_status(app, |s| {
                s.model_state = "ready".into();
                s.model_progress = 100;
                s.model_error = None;
            });
        }
        _ => {
            let msg = [stt.as_ref().err(), fmt.as_ref().err()]
                .iter()
                .flatten()
                .map(|e| format!("{e:#}"))
                .collect::<Vec<_>>()
                .join(" · ");
            tracing::error!("provider load failed: {msg}");
            update_status(app, |s| {
                s.model_state = "error".into();
                s.model_error = Some(msg);
            });
        }
    }
    Providers {
        stt: stt.ok(),
        fmt: fmt.ok(),
    }
}

fn run(app: &AppHandle, store: &Store, rx: crossbeam_channel::Receiver<PipelineMsg>) {
    let keep_days: u32 = store
        .setting_or("history_keep_days", "30")
        .parse()
        .unwrap_or(30);
    if let Ok(n) = store.history_prune(keep_days) {
        if n > 0 {
            tracing::info!("pruned {n} history rows older than {keep_days} days");
        }
    }

    let mut providers = load_providers(app, store);
    update_status(app, |s| s.hotkey_alive = true);

    let mut recorder: Option<Recorder> = None;
    let mut record_ctx = context::AppContext::default();
    let mut locked = false;
    let mut started_at = Instant::now();

    for msg in rx {
        match msg {
            PipelineMsg::ReloadProvider => {
                recorder = None;
                locked = false;
                set_recording(app, false, false);
                providers = load_providers(app, store);
            }
            PipelineMsg::Hotkey(ev) => {
                let toggle_mode = store.setting_or("ptt_mode", "hold") == "toggle";
                match ev {
                    HotkeyEvent::RecordStart => {
                        if recorder.is_some() && (toggle_mode || locked) {
                            // Second press stops (toggle mode or locked hold).
                            locked = false;
                            finish_recording(app, store, &providers, &mut recorder, &record_ctx);
                            continue;
                        }
                        if recorder.is_some() {
                            continue;
                        }
                        if providers.stt.is_none() || providers.fmt.is_none() {
                            providers = load_providers(app, store);
                            if providers.stt.is_none() {
                                continue;
                            }
                        }
                        // Snapshot frontmost app + selection BEFORE recording:
                        // this is the injection target (plan 2.6/2.8).
                        record_ctx = context::snapshot_on_main(app);
                        let device = store.setting_or("input_device", "default");
                        match Recorder::start(Some(&device)) {
                            Ok(r) => {
                                recorder = Some(r);
                                locked = false;
                                started_at = Instant::now();
                                set_recording(app, true, false);
                            }
                            Err(e) => {
                                tracing::error!("failed to start recording: {e:#}");
                                pill(app, "error", &format!("mic: {e:#}"));
                                emit_transcript(app, TranscriptEvent::failed(format!("mic: {e:#}")));
                            }
                        }
                    }
                    HotkeyEvent::Lock => {
                        if recorder.is_some() && !toggle_mode && !locked {
                            locked = true;
                            set_recording(app, true, true);
                        }
                    }
                    HotkeyEvent::RecordStop => {
                        if toggle_mode || locked {
                            continue; // releases are ignored in these states
                        }
                        if recorder.is_some() && started_at.elapsed().as_millis() < 250 {
                            recorder = None;
                            set_recording(app, false, false);
                            continue;
                        }
                        finish_recording(app, store, &providers, &mut recorder, &record_ctx);
                    }
                }
            }
        }
    }
}

fn finish_recording(
    app: &AppHandle,
    store: &Store,
    providers: &Providers,
    recorder: &mut Option<Recorder>,
    record_ctx: &context::AppContext,
) {
    let Some(r) = recorder.take() else { return };
    set_recording(app, false, false);
    let Some(stt) = providers.stt.as_ref() else { return };
    let release = Instant::now();
    pill(app, "transcribing", "");

    let event = match process_utterance(app, store, stt.as_ref(), providers.fmt.as_deref(), r, record_ctx)
    {
        Ok(Some(outcome)) => {
            let inject_err = inject_on_main(app, &outcome.formatted).err();
            let latency = release.elapsed().as_millis() as u64;
            let _ = store.history_insert_full(
                &outcome.raw,
                &outcome.formatted,
                stt.id(),
                &outcome.fmt_provider,
                latency,
            );
            TranscriptEvent {
                injected: inject_err.is_none(),
                error: inject_err.map(|e| format!("{e:#}")).or(outcome.fmt_error),
                latency_ms: latency,
                discarded: false,
                mode: outcome.mode_label,
                formatted: outcome.formatted,
                raw: outcome.raw,
            }
        }
        Ok(None) => TranscriptEvent {
            latency_ms: release.elapsed().as_millis() as u64,
            ..TranscriptEvent::failed(String::new())
        },
        Err(e) => {
            tracing::error!("utterance failed: {e:#}");
            let mut ev = TranscriptEvent::failed(format!("{e:#}"));
            ev.latency_ms = release.elapsed().as_millis() as u64;
            ev
        }
    };
    tracing::info!(
        "utterance ({}): {} ms, discarded={}, injected={}",
        event.mode,
        event.latency_ms,
        event.discarded,
        event.injected
    );
    if event.discarded {
        if let Some(err) = &event.error {
            pill(app, "error", err);
        } else {
            pill(app, "hidden", "");
        }
    } else {
        pill(app, "done", &event.formatted);
    }
    emit_transcript(app, event);
}

struct UtteranceOutcome {
    raw: String,
    formatted: String,
    fmt_provider: String,
    fmt_error: Option<String>,
    mode_label: String,
}

/// stop → resample → VAD → STT → mode decision → formatting → dictionary.
fn process_utterance(
    _app: &AppHandle,
    store: &Store,
    stt: &dyn TranscriptionProvider,
    fmt: Option<&dyn FormattingProvider>,
    recorder: Recorder,
    record_ctx: &context::AppContext,
) -> Result<Option<UtteranceOutcome>> {
    let samples = recorder.stop().context("stopping recorder")?;
    let Some(trimmed) = audio::vad_gate(&samples)? else {
        return Ok(None);
    };

    let dict = store.dict_list().unwrap_or_default();
    let snippets = store.snippets_list().unwrap_or_default();
    let initial_prompt = if dict.is_empty() {
        String::new()
    } else {
        format!(
            "Glossary: {}.",
            dict.iter().map(|d| d.term.as_str()).collect::<Vec<_>>().join(", ")
        )
    };
    let session = SessionCtx {
        language_hint: {
            let l = store.setting_or("language_hint", "auto");
            if l == "auto" { None } else { Some(l) }
        },
        initial_prompt,
    };

    let t_stt = Instant::now();
    let transcript = tauri::async_runtime::block_on(stt.transcribe(
        &AudioBuffer { samples_16k: trimmed },
        &session,
    ))?;
    let raw = transcript.text.trim().to_string();
    tracing::debug!("stt: {} ms", t_stt.elapsed().as_millis());
    if raw.is_empty() {
        return Ok(None);
    }

    // Mode decision (plan 2.8): selection + imperative opener = Command.
    let mode = match &record_ctx.selection {
        Some(sel) if context::looks_like_command(&raw) => Mode::Command { selection: sel.clone() },
        _ => Mode::Dictate,
    };
    let mode_label = match mode {
        Mode::Command { .. } => "command",
        Mode::Dictate => "dictate",
    }
    .to_string();

    let style = store.style_for_app(&record_ctx.bundle_id);
    let fmt_ctx = FormatCtx {
        app_id: record_ctx.bundle_id.clone(),
        style: style.clone(),
        dictionary: dict.clone(),
        snippets,
        mode: mode.clone(),
    };

    // Formatting stage with bypasses (plan 2.6 verbatim, 2.7 short utterance).
    let word_count = raw.split_whitespace().count();
    let skip_llm = matches!(mode, Mode::Dictate) && (style.verbatim || word_count < 5);

    let (formatted, fmt_provider, fmt_error) = if skip_llm || fmt.is_none() {
        (
            providers::passthrough::clean(&raw, &fmt_ctx),
            "passthrough".to_string(),
            None,
        )
    } else {
        let f = fmt.expect("checked above");
        let t_fmt = Instant::now();
        match tauri::async_runtime::block_on(f.format(&raw, &fmt_ctx)) {
            // Guard: a small local model sometimes breaks character and replies
            // conversationally about the task instead of transcribing. Detect
            // that (Dictate only — see looks_like_meta_reply) and fall back to
            // passthrough so the raw dictation is never lost.
            Ok(text)
                if matches!(mode, Mode::Dictate)
                    && providers::looks_like_meta_reply(&text) =>
            {
                tracing::warn!("formatter broke character (meta-reply), using passthrough: {text:?}");
                (
                    providers::passthrough::clean(&raw, &fmt_ctx),
                    "passthrough".to_string(),
                    Some("formatter broke character — used passthrough".into()),
                )
            }
            Ok(text) if !text.trim().is_empty() => {
                tracing::debug!("fmt ({}): {} ms", f.id(), t_fmt.elapsed().as_millis());
                (text.trim().to_string(), f.id().to_string(), None)
            }
            Ok(_) => (
                providers::passthrough::clean(&raw, &fmt_ctx),
                "passthrough".to_string(),
                Some("formatter returned empty text — used passthrough".into()),
            ),
            Err(e) => {
                // Graceful degradation (CLAUDE.md invariant): raw is never lost.
                tracing::warn!("formatter failed, falling back to passthrough: {e:#}");
                if matches!(mode, Mode::Command { .. }) {
                    return Err(e.context("Command Mode rewrite failed — selection unchanged"));
                }
                (
                    providers::passthrough::clean(&raw, &fmt_ctx),
                    "passthrough".to_string(),
                    Some(format!("{e:#}")),
                )
            }
        }
    };

    let formatted = store::apply_dictionary(&formatted, &dict);
    if formatted.is_empty() {
        return Ok(None);
    }
    Ok(Some(UtteranceOutcome {
        raw,
        formatted,
        fmt_provider,
        fmt_error,
        mode_label,
    }))
}

/// Runs the paste on the main thread (mandatory — see inject.rs docs).
fn inject_on_main(app: &AppHandle, text: &str) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let text = text.to_string();
    app.run_on_main_thread(move || {
        let _ = tx.send(inject::inject(&text));
    })
    .context("scheduling injection on main thread")?;
    rx.recv_timeout(Duration::from_secs(5))
        .context("injection timed out")?
}

fn set_recording(app: &AppHandle, rec: bool, locked: bool) {
    update_status(app, |s| s.recording = rec);
    let _ = app.emit("pipeline://recording", rec);
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_title(if rec { Some("🔴") } else { None::<&str> });
    }
    if rec {
        pill(app, if locked { "locked" } else { "listening" }, "");
    }
}

/// Drives the floating pill overlay (user request; plan 0.8 indicator).
/// States: listening | locked | transcribing | done | error | hidden.
///
/// The pill WINDOW stays visible for the app's whole lifetime (see lib.rs) —
/// only the DOM content comes and goes. Hiding the NSWindow lets macOS
/// suspend its WKWebView, after which emitted events are never processed.
fn pill(app: &AppHandle, state: &str, detail: &str) {
    static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;

    match app.get_webview_window("pill") {
        Some(w) => {
            // Re-assert position + visibility (displays change, users minimize).
            position_pill(app, &w);
            if !w.is_visible().unwrap_or(false) {
                let _ = w.show();
            }
        }
        None => tracing::warn!("pill window not found — overlay disabled"),
    }
    let _ = app.emit("pipeline://pill", json!({ "state": state, "detail": detail }));

    // Terminal states linger briefly, then clear — unless a newer state
    // (e.g. the next recording) has already superseded this one.
    if matches!(state, "done" | "error") {
        let app2 = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1400));
            if GENERATION.load(std::sync::atomic::Ordering::SeqCst) == generation {
                let _ = app2.emit("pipeline://pill", json!({ "state": "hidden", "detail": "" }));
            }
        });
    }
}

pub fn position_pill(app: &AppHandle, w: &tauri::WebviewWindow) {
    if let (Ok(Some(monitor)), Ok(size)) = (app.primary_monitor(), w.outer_size()) {
        // Work area excludes the menu bar AND the Dock — a pill placed inside
        // the Dock's zone is drawn over by it (Dock window level 20 beats any
        // normal floating window).
        let wa = monitor.work_area();
        let x = wa.position.x + ((wa.size.width as i32 - size.width as i32) / 2);
        let y = wa.position.y + wa.size.height as i32 - size.height as i32 - 24;
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

fn emit_transcript(app: &AppHandle, ev: TranscriptEvent) {
    let _ = app.emit("pipeline://transcript", &ev);
}

/// Shared by the `transcribe_wav` dev command and the `transcribe_file`
/// example: wav → 16 kHz mono → VAD → provider.
pub fn transcribe_wav_file(engine: &str, model_path: &Path, wav_path: &Path) -> Result<String> {
    let t_load = Instant::now();
    let provider: Box<dyn TranscriptionProvider> = match engine {
        "whisper-cpp" => Box::new(crate::providers::whisper_cpp::WhisperCpp::load(model_path)?),
        "parakeet" => Box::new(crate::providers::parakeet::Parakeet::load(model_path)?),
        other => anyhow::bail!("unsupported engine '{other}' for wav transcription"),
    };
    tracing::info!("model load: {} ms", t_load.elapsed().as_millis());

    let mut reader = hound::WavReader::open(wav_path).context("opening wav")?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<_, _>>()?
        }
    };
    let mono: Vec<f32> = if spec.channels > 1 {
        samples
            .chunks_exact(spec.channels as usize)
            .map(|f| f.iter().sum::<f32>() / f.len() as f32)
            .collect()
    } else {
        samples
    };
    let t_prep = Instant::now();
    let resampled = audio::resample_to_16k(&mono, spec.sample_rate)?;
    let Some(trimmed) = audio::vad_gate(&resampled)? else {
        anyhow::bail!("VAD found no speech in {}", wav_path.display());
    };
    tracing::info!("resample+VAD: {} ms", t_prep.elapsed().as_millis());
    let t_stt = Instant::now();
    let result = tauri::async_runtime::block_on(provider.transcribe(
        &AudioBuffer { samples_16k: trimmed },
        &SessionCtx {
            language_hint: None,
            initial_prompt: String::new(),
        },
    ))?;
    tracing::info!("inference: {} ms", t_stt.elapsed().as_millis());
    Ok(result.text)
}
