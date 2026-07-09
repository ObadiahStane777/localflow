import { useCallback, useEffect, useState } from "react";
import {
  checkAccessibilityPermission,
  requestAccessibilityPermission,
  checkMicrophonePermission,
  requestMicrophonePermission,
  checkInputMonitoringPermission,
  requestInputMonitoringPermission,
} from "tauri-plugin-macos-permissions-api";
import { type AppStatus, type TranscriptEvent } from "../lib/ipc";

interface PermState {
  microphone: boolean;
  accessibility: boolean;
  inputMonitoring: boolean;
}

function Dot({ ok }: { ok: boolean }) {
  return (
    <span
      className={`inline-block h-2.5 w-2.5 rounded-full ${ok ? "bg-emerald-400" : "bg-red-400"}`}
    />
  );
}

function PermissionRow({
  name,
  why,
  granted,
  onFix,
}: {
  name: string;
  why: string;
  granted: boolean;
  onFix: () => void;
}) {
  return (
    <div className="flex items-center justify-between rounded-lg bg-neutral-900 px-4 py-3">
      <div className="flex items-center gap-3">
        <Dot ok={granted} />
        <div>
          <div className="text-sm font-medium">{name}</div>
          <div className="text-xs text-neutral-400">{why}</div>
        </div>
      </div>
      {!granted && (
        <button
          onClick={onFix}
          className="rounded-md bg-neutral-700 px-3 py-1.5 text-xs font-medium hover:bg-neutral-600"
        >
          Grant…
        </button>
      )}
    </div>
  );
}

export default function Home({
  status,
  lastTranscript,
}: {
  status: AppStatus | null;
  lastTranscript: TranscriptEvent | null;
}) {
  const [perms, setPerms] = useState<PermState>({
    microphone: false,
    accessibility: false,
    inputMonitoring: false,
  });

  const refreshPerms = useCallback(async () => {
    const [microphone, accessibility, inputMonitoring] = await Promise.all([
      checkMicrophonePermission(),
      checkAccessibilityPermission(),
      checkInputMonitoringPermission(),
    ]);
    setPerms({ microphone, accessibility, inputMonitoring });
  }, []);

  useEffect(() => {
    refreshPerms();
    const onFocus = () => refreshPerms();
    window.addEventListener("focus", onFocus);
    const iv = setInterval(onFocus, 3000);
    return () => {
      window.removeEventListener("focus", onFocus);
      clearInterval(iv);
    };
  }, [refreshPerms]);

  return (
    <div className="flex flex-col gap-5">
      <section className="flex flex-col gap-2">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
          Permissions
        </h2>
        <PermissionRow
          name="Microphone"
          why="Captures your voice while the hotkey is held"
          granted={perms.microphone}
          onFix={() => requestMicrophonePermission().then(refreshPerms)}
        />
        <PermissionRow
          name="Accessibility"
          why="Pastes the transcript into the focused app (⌘V)"
          granted={perms.accessibility}
          onFix={() => requestAccessibilityPermission().then(refreshPerms)}
        />
        <PermissionRow
          name="Input Monitoring"
          why="Detects the global push-to-talk hotkey"
          granted={perms.inputMonitoring}
          onFix={() => requestInputMonitoringPermission().then(refreshPerms)}
        />
      </section>

      <section className="flex flex-col gap-2">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
          Engine
        </h2>
        <div className="rounded-lg bg-neutral-900 px-4 py-3 text-sm">
          {status === null && <span className="text-neutral-400">Checking…</span>}
          {status?.modelState === "loading" && <span>Loading {status.sttProvider}…</span>}
          {status?.modelState === "ready" && (
            <span className="text-emerald-300">
              {status.sttProvider} ready{" "}
              {status.sttProvider !== "openai" ? "— offline" : "(cloud, BYOK)"}
            </span>
          )}
          {status?.modelState === "error" && (
            <span className="text-red-300">{status.modelError}</span>
          )}
        </div>
      </section>

      <section className="flex flex-col gap-2">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
          Last dictation
        </h2>
        <div className="min-h-16 rounded-lg bg-neutral-900 px-4 py-3 text-sm">
          {lastTranscript === null && (
            <span className="text-neutral-500">Nothing yet. Hold your hotkey and speak.</span>
          )}
          {lastTranscript?.discarded && (
            <span className="text-neutral-400">
              {lastTranscript.error ?? "No speech detected (discarded by VAD)."}
            </span>
          )}
          {lastTranscript && !lastTranscript.discarded && (
            <div className="flex flex-col gap-1">
              <span className="select-text whitespace-pre-wrap">
                {lastTranscript.formatted || lastTranscript.raw}
              </span>
              {lastTranscript.formatted &&
                lastTranscript.formatted !== lastTranscript.raw && (
                  <span className="select-text text-xs text-neutral-500">
                    raw: {lastTranscript.raw}
                  </span>
                )}
              <span className="text-xs text-neutral-500">
                {lastTranscript.latencyMs} ms
                {lastTranscript.mode === "command" ? " · command mode" : ""}
                {lastTranscript.injected ? " · injected" : ""}
                {lastTranscript.error ? ` · ${lastTranscript.error}` : ""}
              </span>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}
