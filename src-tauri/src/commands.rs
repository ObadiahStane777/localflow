//! ALL Tauri commands/events — the single surface between core and UI.

use crate::pipeline::{PipelineHandle, PipelineMsg};
use crate::store::{DictEntry, HistoryRow, Store};
use anyhow::Result;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

// ---- Shared status (mirrored in src/lib/ipc.ts) ----

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub model_state: String, // missing | loading | ready | error
    pub model_progress: u8,
    pub model_error: Option<String>,
    pub recording: bool,
    pub hotkey_alive: bool,
    pub stt_provider: String,
}

impl Default for AppStatus {
    fn default() -> Self {
        Self {
            model_state: "loading".into(),
            model_progress: 0,
            model_error: None,
            recording: false,
            hotkey_alive: false,
            stt_provider: "whisper-cpp".into(),
        }
    }
}

pub struct AppState {
    pub status: Mutex<AppStatus>,
    pub store: Arc<Store>,
    pub pipeline: Mutex<Option<PipelineHandle>>,
}

/// Mutates shared status and pushes it to the UI.
pub fn update_status(app: &AppHandle, f: impl FnOnce(&mut AppStatus)) {
    let state: State<AppState> = app.state();
    let snapshot = {
        let mut guard = state.status.lock().expect("status poisoned");
        f(&mut guard);
        guard.clone()
    };
    let _ = app.emit("model://status", &snapshot);
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEvent {
    pub raw: String,
    pub formatted: String,
    pub latency_ms: u64,
    pub discarded: bool,
    pub injected: bool,
    pub error: Option<String>,
    pub mode: String, // "dictate" | "command"
}

impl TranscriptEvent {
    /// Discarded/failed utterance (empty error = silent VAD discard).
    pub fn failed(error: String) -> Self {
        Self {
            raw: String::new(),
            formatted: String::new(),
            latency_ms: 0,
            discarded: true,
            injected: false,
            error: if error.is_empty() { None } else { Some(error) },
            mode: "dictate".into(),
        }
    }
}

fn reload_pipeline(state: &State<'_, AppState>) {
    if let Some(h) = state.pipeline.lock().expect("pipeline poisoned").as_ref() {
        let _ = h.tx.send(PipelineMsg::ReloadProvider);
    }
}

// ---- Keychain (plan 1.3: key never touches SQLite or plaintext config) ----

const KEYCHAIN_SERVICE: &str = "com.localflow.app";
const KEYCHAIN_ACCOUNT: &str = "openai_api_key";

pub fn read_api_key() -> Result<Option<String>> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)?;
    match entry.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// ---- Status & settings ----

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> AppStatus {
    state.status.lock().expect("status poisoned").clone()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub stt_provider: String,
    pub whisper_model: String,
    pub openai_stt_model: String,
    pub fmt_provider: String, // "auto" | "ollama" | "openai" | "passthrough"
    pub ollama_model: String,
    pub openai_fmt_model: String,
    pub hotkey_keycode: i64,
    pub ptt_mode: String, // "hold" | "toggle"
    pub input_device: String,
    pub language_hint: String,
    pub history_keep_days: u32,
    pub api_key_last4: Option<String>,
    pub input_devices: Vec<String>,
    pub hotkey_choices: Vec<(String, i64)>,
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    let s = &state.store;
    let last4 = read_api_key()
        .ok()
        .flatten()
        .map(|k| k.chars().rev().take(4).collect::<String>().chars().rev().collect());
    Settings {
        stt_provider: s.setting_or("stt_provider", "whisper-cpp"),
        whisper_model: s.setting_or("whisper_model", "whisper-small"),
        openai_stt_model: s.setting_or("openai_stt_model", "gpt-4o-mini-transcribe"),
        fmt_provider: s.setting_or("fmt_provider", "auto"),
        ollama_model: s.setting_or("ollama_model", "qwen2.5:3b"),
        openai_fmt_model: s.setting_or("openai_fmt_model", "gpt-4o-mini"),
        hotkey_keycode: s.setting_or("hotkey_keycode", "54").parse().unwrap_or(54),
        ptt_mode: s.setting_or("ptt_mode", "hold"),
        input_device: s.setting_or("input_device", "default"),
        language_hint: s.setting_or("language_hint", "auto"),
        history_keep_days: s.setting_or("history_keep_days", "30").parse().unwrap_or(30),
        api_key_last4: last4,
        input_devices: crate::audio::input_devices(),
        hotkey_choices: crate::hotkey::HOTKEY_CHOICES
            .iter()
            .map(|(label, code, _)| (label.to_string(), *code))
            .collect(),
    }
}

#[tauri::command]
pub fn set_setting(state: State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    const ALLOWED: &[&str] = &[
        "stt_provider",
        "whisper_model",
        "openai_stt_model",
        "fmt_provider",
        "ollama_model",
        "openai_fmt_model",
        "hotkey_keycode",
        "ptt_mode",
        "input_device",
        "language_hint",
        "history_keep_days",
    ];
    if !ALLOWED.contains(&key.as_str()) {
        return Err(format!("unknown setting '{key}'"));
    }
    state
        .store
        .set_setting(&key, &value)
        .map_err(|e| e.to_string())?;
    match key.as_str() {
        "hotkey_keycode" => {
            if let Some(h) = state.pipeline.lock().expect("pipeline poisoned").as_ref() {
                h.hotkey_keycode
                    .store(value.parse().unwrap_or(54), Ordering::Relaxed);
            }
        }
        "stt_provider" | "whisper_model" | "openai_stt_model" | "fmt_provider"
        | "ollama_model" | "openai_fmt_model" => reload_pipeline(&state),
        _ => {}
    }
    Ok(())
}

#[tauri::command]
pub fn set_api_key(state: State<'_, AppState>, key: String) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|e| e.to_string())?;
    if key.is_empty() {
        let _ = entry.delete_credential();
    } else {
        entry.set_password(&key).map_err(|e| e.to_string())?;
    }
    if state.store.setting_or("stt_provider", "whisper-cpp") == "openai" {
        reload_pipeline(&state);
    }
    Ok(())
}

// ---- Models ----

#[tauri::command]
pub fn list_models(app: AppHandle) -> Result<Vec<crate::models::ModelInfo>, String> {
    let dir = crate::models::models_dir(&app).map_err(|e| e.to_string())?;
    Ok(crate::models::catalog_info(&dir))
}

#[tauri::command]
pub async fn download_model(app: AppHandle, model_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let res = crate::models::download_model(&app, &model_id).map_err(|e| format!("{e:#}"));
        if let Err(err) = &res {
            let _ = app.emit(
                "model://download",
                crate::models::DownloadProgress {
                    model_id: model_id.clone(),
                    percent: 0,
                    done: true,
                    error: Some(err.clone()),
                },
            );
        }
        res
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn delete_model(app: AppHandle, state: State<'_, AppState>, model_id: String) -> Result<(), String> {
    crate::models::delete_model(&app, &model_id).map_err(|e| format!("{e:#}"))?;
    reload_pipeline(&state);
    Ok(())
}

// ---- History ----

#[tauri::command]
pub fn history_list(
    state: State<'_, AppState>,
    query: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<HistoryRow>, String> {
    state
        .store
        .history_list(query.as_deref(), limit.unwrap_or(100))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn history_delete(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.store.history_delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn history_clear(state: State<'_, AppState>) -> Result<(), String> {
    state.store.history_clear().map_err(|e| e.to_string())
}

// ---- Dictionary ----

#[tauri::command]
pub fn dict_list(state: State<'_, AppState>) -> Result<Vec<DictEntry>, String> {
    state.store.dict_list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn dict_add(
    state: State<'_, AppState>,
    term: String,
    spoken_variants: String,
) -> Result<(), String> {
    if term.trim().is_empty() {
        return Err("term cannot be empty".into());
    }
    state
        .store
        .dict_add(term.trim(), spoken_variants.trim(), true)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn dict_delete(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.store.dict_delete(id).map_err(|e| e.to_string())
}

// ---- Snippets (plan 2.9) ----

#[tauri::command]
pub fn snippets_list(state: State<'_, AppState>) -> Result<Vec<crate::store::Snippet>, String> {
    state.store.snippets_list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn snippet_add(
    state: State<'_, AppState>,
    trigger: String,
    expansion: String,
) -> Result<(), String> {
    if trigger.trim().is_empty() || expansion.trim().is_empty() {
        return Err("trigger and expansion are both required".into());
    }
    state
        .store
        .snippet_add(trigger.trim(), expansion.trim())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn snippet_delete(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.store.snippet_delete(id).map_err(|e| e.to_string())
}

// ---- Styles (plan 2.6) ----

#[tauri::command]
pub fn styles_list(state: State<'_, AppState>) -> Result<Vec<crate::store::StyleProfile>, String> {
    state.store.styles_list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn style_update(
    state: State<'_, AppState>,
    id: i64,
    match_pattern: String,
    instructions: String,
) -> Result<(), String> {
    state
        .store
        .style_update(id, match_pattern.trim(), instructions.trim())
        .map_err(|e| e.to_string())
}

// ---- Ollama status (plan 2.2 UX) ----

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaStatus {
    pub server_up: bool,
    pub model_available: bool,
    pub model: String,
}

#[tauri::command]
pub async fn ollama_status(app: AppHandle) -> Result<OllamaStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state: State<AppState> = app.state();
        let url = state.store.setting_or("ollama_url", "http://localhost:11434");
        let model = state.store.setting_or("ollama_model", "qwen2.5:3b");
        let f = crate::providers::ollama_fmt::OllamaFormatter::new(url, model.clone());
        let server_up = f.server_up();
        Ok(OllamaStatus {
            server_up,
            model_available: server_up && f.model_available(),
            model,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- Dev harness ----

/// Run a wav file through resample → VAD → active local engine without
/// touching the mic or hotkey.
#[tauri::command]
pub async fn transcribe_wav(app: AppHandle, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state: State<AppState> = app.state();
        let dir = crate::models::models_dir(&app).map_err(|e| e.to_string())?;
        let provider = state.store.setting_or("stt_provider", "whisper-cpp");
        let model_path = match provider.as_str() {
            "parakeet" => dir.join(crate::models::PARAKEET_DIR),
            _ => {
                let m = state.store.setting_or("whisper_model", "whisper-small");
                crate::models::whisper_model_path(&dir, &m).map_err(|e| e.to_string())?
            }
        };
        let engine = if provider == "parakeet" { "parakeet" } else { "whisper-cpp" };
        crate::pipeline::transcribe_wav_file(engine, &model_path, std::path::Path::new(&path))
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}
