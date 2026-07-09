import { useCallback, useEffect, useState } from "react";
import {
  getSettings,
  getStatus,
  onModelStatus,
  onRecording,
  onTranscript,
  type AppStatus,
  type Settings,
  type TranscriptEvent,
} from "./lib/ipc";
import Home from "./views/Home";
import Models from "./views/Models";
import History from "./views/History";
import Dictionary from "./views/Dictionary";
import Snippets from "./views/Snippets";
import Styles from "./views/Styles";
import SettingsView from "./views/SettingsView";

const TABS = [
  "Home",
  "Models",
  "History",
  "Dictionary",
  "Snippets",
  "Styles",
  "Settings",
] as const;
type Tab = (typeof TABS)[number];

export default function App() {
  const [tab, setTab] = useState<Tab>("Home");
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [recording, setRecording] = useState(false);
  const [lastTranscript, setLastTranscript] = useState<TranscriptEvent | null>(null);
  const [historyKey, setHistoryKey] = useState(0);

  const refreshSettings = useCallback(() => {
    getSettings().then(setSettings).catch(console.error);
  }, []);

  useEffect(() => {
    getStatus().then(setStatus).catch(console.error);
    refreshSettings();
    const unlisteners = [
      onModelStatus(setStatus),
      onRecording(setRecording),
      onTranscript((t) => {
        setLastTranscript(t);
        setHistoryKey((k) => k + 1);
      }),
    ];
    return () => {
      unlisteners.forEach((p) => p.then((u) => u()));
    };
  }, [refreshSettings]);

  const ready = status?.modelState === "ready";

  return (
    <div className="mx-auto flex max-w-xl flex-col gap-4 p-6">
      <header className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold tracking-tight">LocalFlow</h1>
          <p className="text-xs text-neutral-400">
            Hold your hotkey anywhere, speak, release — text appears at your cursor.
          </p>
        </div>
        <div
          className={`rounded-full px-3 py-1 text-xs font-medium ${
            recording
              ? "bg-red-500/20 text-red-300"
              : ready
                ? "bg-emerald-500/20 text-emerald-300"
                : "bg-amber-500/20 text-amber-300"
          }`}
        >
          {recording ? "● Recording" : ready ? "Ready" : "Setup needed"}
        </div>
      </header>

      <nav className="flex gap-1 rounded-lg bg-neutral-900 p-1">
        {TABS.map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={`flex-1 rounded-md px-3 py-1.5 text-xs font-medium transition ${
              tab === t ? "bg-neutral-700 text-white" : "text-neutral-400 hover:text-white"
            }`}
          >
            {t}
          </button>
        ))}
      </nav>

      {tab === "Home" && <Home status={status} lastTranscript={lastTranscript} />}
      {tab === "Models" && (
        <Models settings={settings} refreshSettings={refreshSettings} />
      )}
      {tab === "History" && <History refreshKey={historyKey} />}
      {tab === "Dictionary" && <Dictionary />}
      {tab === "Snippets" && <Snippets />}
      {tab === "Styles" && <Styles />}
      {tab === "Settings" && (
        <SettingsView settings={settings} refreshSettings={refreshSettings} />
      )}
    </div>
  );
}
