import { useEffect, useState } from "react";
import { isEnabled, enable, disable } from "@tauri-apps/plugin-autostart";
import { setApiKey, setSetting, type Settings } from "../lib/ipc";

const PRESETS: {
  id: string;
  label: string;
  desc: string;
  apply: { stt: string; fmt: string };
}[] = [
  {
    id: "fully-local",
    label: "Fully Local",
    desc: "Whisper + Ollama on-device. Nothing leaves your Mac.",
    apply: { stt: "whisper-cpp", fmt: "ollama" },
  },
  {
    id: "fast-local",
    label: "Fast Local",
    desc: "Parakeet + regex cleanup — fastest, English only.",
    apply: { stt: "parakeet", fmt: "passthrough" },
  },
  {
    id: "hybrid",
    label: "Hybrid",
    desc: "Local speech-to-text, GPT formatting. Audio never leaves; text does.",
    apply: { stt: "whisper-cpp", fmt: "openai" },
  },
  {
    id: "full-cloud",
    label: "Full Cloud",
    desc: "OpenAI STT + GPT formatting — highest accuracy, needs API key.",
    apply: { stt: "openai", fmt: "openai" },
  },
];

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between rounded-lg bg-neutral-900 px-4 py-3">
      <span className="text-sm">{label}</span>
      {children}
    </div>
  );
}

const selectCls =
  "rounded-md bg-neutral-800 px-2 py-1.5 text-xs outline-none focus:ring-1 focus:ring-emerald-500";

export default function SettingsView({
  settings,
  refreshSettings,
}: {
  settings: Settings | null;
  refreshSettings: () => void;
}) {
  const [autostart, setAutostart] = useState(false);
  const [keyDraft, setKeyDraft] = useState("");
  const [keySaved, setKeySaved] = useState(false);

  useEffect(() => {
    isEnabled().then(setAutostart).catch(console.error);
  }, []);

  if (!settings) return <p className="text-sm text-neutral-500">Loading…</p>;

  const set = (k: string, v: string) => setSetting(k, v).then(refreshSettings);

  const applyPreset = async (p: (typeof PRESETS)[number]) => {
    await setSetting("stt_provider", p.apply.stt);
    await setSetting("fmt_provider", p.apply.fmt);
    refreshSettings();
  };

  const activePreset =
    PRESETS.find(
      (p) =>
        p.apply.stt === settings.sttProvider &&
        p.apply.fmt === settings.fmtProvider,
    )?.id ?? null;

  return (
    <div className="flex flex-col gap-5">
      <section className="flex flex-col gap-2">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
          Preset
        </h2>
        <div className="grid grid-cols-2 gap-2">
          {PRESETS.map((p) => (
            <button
              key={p.id}
              onClick={() => applyPreset(p)}
              className={`rounded-lg px-3 py-2.5 text-left transition ${
                activePreset === p.id
                  ? "bg-emerald-500/20 ring-1 ring-emerald-500"
                  : "bg-neutral-900 hover:bg-neutral-800"
              }`}
            >
              <div className="text-xs font-semibold">{p.label}</div>
              <div className="mt-0.5 text-[10px] leading-tight text-neutral-400">
                {p.desc}
              </div>
            </button>
          ))}
        </div>
      </section>

      <section className="flex flex-col gap-2">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
          Formatting (the AI cleanup layer)
        </h2>
        <Row label="Formatter">
          <select
            value={settings.fmtProvider}
            onChange={(e) => set("fmt_provider", e.target.value)}
            className={selectCls}
          >
            <option value="auto">Auto (Ollama if running, else basic)</option>
            <option value="ollama">Ollama (local LLM)</option>
            <option value="openai">OpenAI GPT</option>
            <option value="passthrough">Basic cleanup only (no LLM)</option>
          </select>
        </Row>
        <Row label="Ollama model">
          <input
            value={settings.ollamaModel}
            onChange={(e) => set("ollama_model", e.target.value)}
            className="w-44 rounded-md bg-neutral-800 px-2 py-1.5 text-xs outline-none focus:ring-1 focus:ring-emerald-500"
          />
        </Row>
        <Row label="OpenAI formatting model">
          <input
            value={settings.openaiFmtModel}
            onChange={(e) => set("openai_fmt_model", e.target.value)}
            className="w-44 rounded-md bg-neutral-800 px-2 py-1.5 text-xs outline-none focus:ring-1 focus:ring-emerald-500"
          />
        </Row>
      </section>

      <section className="flex flex-col gap-2">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
          OpenAI (bring your own key)
        </h2>
        <div className="flex flex-col gap-2 rounded-lg bg-neutral-900 px-4 py-3">
          <div className="flex items-center gap-2">
            <input
              type="password"
              value={keyDraft}
              onChange={(e) => setKeyDraft(e.target.value)}
              placeholder={
                settings.apiKeyLast4
                  ? `Key saved (…${settings.apiKeyLast4}) — paste to replace`
                  : "sk-…"
              }
              className="flex-1 rounded-md bg-neutral-800 px-3 py-2 text-sm outline-none placeholder:text-neutral-600 focus:ring-1 focus:ring-emerald-500"
            />
            <button
              onClick={() =>
                setApiKey(keyDraft).then(() => {
                  setKeyDraft("");
                  setKeySaved(true);
                  setTimeout(() => setKeySaved(false), 1500);
                  refreshSettings();
                })
              }
              disabled={!keyDraft}
              className="rounded-md bg-emerald-700 px-3 py-2 text-xs font-medium hover:bg-emerald-600 disabled:opacity-40"
            >
              {keySaved ? "Saved ✓" : "Save"}
            </button>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-xs text-neutral-500">
              Stored in the macOS Keychain, never on disk.
            </span>
            <select
              value={settings.openaiSttModel}
              onChange={(e) => set("openai_stt_model", e.target.value)}
              className={selectCls}
            >
              <option value="gpt-4o-transcribe">gpt-4o-transcribe</option>
              <option value="gpt-4o-mini-transcribe">gpt-4o-mini-transcribe</option>
              <option value="whisper-1">whisper-1</option>
            </select>
          </div>
        </div>
      </section>

      <section className="flex flex-col gap-2">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
          Input
        </h2>
        <Row label="Hotkey">
          <select
            value={settings.hotkeyKeycode}
            onChange={(e) => set("hotkey_keycode", e.target.value)}
            className={selectCls}
          >
            {settings.hotkeyChoices.map(([label, code]) => (
              <option key={code} value={code}>
                {label}
              </option>
            ))}
          </select>
        </Row>
        <Row label="Mode">
          <select
            value={settings.pttMode}
            onChange={(e) => set("ptt_mode", e.target.value)}
            className={selectCls}
          >
            <option value="hold">Hold to talk</option>
            <option value="toggle">Press to toggle</option>
          </select>
        </Row>
        <Row label="Microphone">
          <select
            value={settings.inputDevice}
            onChange={(e) => set("input_device", e.target.value)}
            className={selectCls}
          >
            <option value="default">System default</option>
            {settings.inputDevices.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>
        </Row>
      </section>

      <section className="flex flex-col gap-2">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
          General
        </h2>
        <Row label="Launch at login">
          <input
            type="checkbox"
            checked={autostart}
            onChange={async (e) => {
              if (e.target.checked) await enable();
              else await disable();
              setAutostart(await isEnabled());
            }}
            className="h-4 w-4 accent-emerald-500"
          />
        </Row>
        <Row label="Keep history for">
          <select
            value={settings.historyKeepDays}
            onChange={(e) => set("history_keep_days", e.target.value)}
            className={selectCls}
          >
            <option value="7">7 days</option>
            <option value="30">30 days</option>
            <option value="90">90 days</option>
            <option value="365">1 year</option>
          </select>
        </Row>
      </section>
    </div>
  );
}
