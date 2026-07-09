import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ---- Types mirrored from src-tauri/src/commands.rs ----

export type ModelState = "missing" | "loading" | "ready" | "error";

export interface AppStatus {
  modelState: ModelState;
  modelProgress: number;
  modelError: string | null;
  recording: boolean;
  hotkeyAlive: boolean;
  sttProvider: string;
}

export interface TranscriptEvent {
  raw: string;
  formatted: string;
  latencyMs: number;
  discarded: boolean;
  injected: boolean;
  error: string | null;
  mode: "dictate" | "command";
}

export interface Settings {
  sttProvider: string;
  whisperModel: string;
  openaiSttModel: string;
  fmtProvider: "auto" | "ollama" | "openai" | "passthrough";
  ollamaModel: string;
  openaiFmtModel: string;
  hotkeyKeycode: number;
  pttMode: "hold" | "toggle";
  inputDevice: string;
  languageHint: string;
  historyKeepDays: number;
  apiKeyLast4: string | null;
  inputDevices: string[];
  hotkeyChoices: [string, number][];
}

export interface SnippetRow {
  id: number;
  trigger: string;
  expansion: string;
}

export interface StyleProfileRow {
  id: number;
  matchPattern: string;
  name: string;
  instructions: string;
  verbatim: boolean;
}

export interface OllamaStatus {
  serverUp: boolean;
  modelAvailable: boolean;
  model: string;
}

export interface ModelInfo {
  id: string;
  display: string;
  engine: "whisper-cpp" | "parakeet";
  totalSize: number;
  speedHint: string;
  qualityHint: string;
  englishOnly: boolean;
  installed: boolean;
}

export interface DownloadProgress {
  modelId: string;
  percent: number;
  done: boolean;
  error: string | null;
}

export interface HistoryRow {
  id: number;
  ts: number;
  appId: string;
  raw: string;
  formatted: string;
  sttProvider: string;
  latencyMs: number;
}

export interface DictEntry {
  id: number;
  term: string;
  spokenVariants: string;
  caseSensitive: boolean;
}

// ---- Commands ----

export const getStatus = () => invoke<AppStatus>("get_status");
export const getSettings = () => invoke<Settings>("get_settings");
export const setSetting = (key: string, value: string) =>
  invoke<void>("set_setting", { key, value });
export const setApiKey = (key: string) => invoke<void>("set_api_key", { key });

export const listModels = () => invoke<ModelInfo[]>("list_models");
export const downloadModel = (modelId: string) =>
  invoke<void>("download_model", { modelId });
export const deleteModel = (modelId: string) =>
  invoke<void>("delete_model", { modelId });

export const historyList = (query?: string, limit?: number) =>
  invoke<HistoryRow[]>("history_list", { query, limit });
export const historyDelete = (id: number) => invoke<void>("history_delete", { id });
export const historyClear = () => invoke<void>("history_clear");

export const dictList = () => invoke<DictEntry[]>("dict_list");
export const dictAdd = (term: string, spokenVariants: string) =>
  invoke<void>("dict_add", { term, spokenVariants });
export const dictDelete = (id: number) => invoke<void>("dict_delete", { id });

export const snippetsList = () => invoke<SnippetRow[]>("snippets_list");
export const snippetAdd = (trigger: string, expansion: string) =>
  invoke<void>("snippet_add", { trigger, expansion });
export const snippetDelete = (id: number) => invoke<void>("snippet_delete", { id });

export const stylesList = () => invoke<StyleProfileRow[]>("styles_list");
export const styleUpdate = (id: number, matchPattern: string, instructions: string) =>
  invoke<void>("style_update", { id, matchPattern, instructions });

export const ollamaStatus = () => invoke<OllamaStatus>("ollama_status");

export const transcribeWav = (path: string) =>
  invoke<string>("transcribe_wav", { path });

// ---- Events ----

export const onModelStatus = (cb: (s: AppStatus) => void): Promise<UnlistenFn> =>
  listen<AppStatus>("model://status", (e) => cb(e.payload));

export const onDownloadProgress = (
  cb: (p: DownloadProgress) => void,
): Promise<UnlistenFn> =>
  listen<DownloadProgress>("model://download", (e) => cb(e.payload));

export const onRecording = (cb: (rec: boolean) => void): Promise<UnlistenFn> =>
  listen<boolean>("pipeline://recording", (e) => cb(e.payload));

export const onTranscript = (cb: (t: TranscriptEvent) => void): Promise<UnlistenFn> =>
  listen<TranscriptEvent>("pipeline://transcript", (e) => cb(e.payload));
