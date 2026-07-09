import { useCallback, useEffect, useState } from "react";
import {
  deleteModel,
  downloadModel,
  listModels,
  onDownloadProgress,
  setSetting,
  type ModelInfo,
  type Settings,
} from "../lib/ipc";

function fmtSize(bytes: number): string {
  if (bytes > 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  return `${Math.round(bytes / 1_000_000)} MB`;
}

export default function Models({
  settings,
  refreshSettings,
}: {
  settings: Settings | null;
  refreshSettings: () => void;
}) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [progress, setProgress] = useState<Record<string, number>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});

  const refresh = useCallback(() => {
    listModels().then(setModels).catch(console.error);
  }, []);

  useEffect(() => {
    refresh();
    const un = onDownloadProgress((p) => {
      if (p.error) {
        setErrors((e) => ({ ...e, [p.modelId]: p.error! }));
        setProgress((pr) => {
          const { [p.modelId]: _, ...rest } = pr;
          return rest;
        });
      } else if (p.done) {
        setProgress((pr) => {
          const { [p.modelId]: _, ...rest } = pr;
          return rest;
        });
        refresh();
      } else {
        setProgress((pr) => ({ ...pr, [p.modelId]: p.percent }));
      }
    });
    return () => {
      un.then((u) => u());
    };
  }, [refresh]);

  const activate = async (m: ModelInfo) => {
    if (m.engine === "whisper-cpp") {
      await setSetting("whisper_model", m.id);
      await setSetting("stt_provider", "whisper-cpp");
    } else {
      await setSetting("stt_provider", "parakeet");
    }
    refreshSettings();
  };

  const isActive = (m: ModelInfo) => {
    if (!settings) return false;
    if (m.engine === "parakeet") return settings.sttProvider === "parakeet";
    return (
      settings.sttProvider === "whisper-cpp" && settings.whisperModel === m.id
    );
  };

  return (
    <div className="flex flex-col gap-2">
      <h2 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
        Local models
      </h2>
      {models.map((m) => (
        <div key={m.id} className="rounded-lg bg-neutral-900 px-4 py-3">
          <div className="flex items-center justify-between">
            <div>
              <div className="text-sm font-medium">
                {m.display}{" "}
                <span className="text-xs text-neutral-500">
                  {fmtSize(m.totalSize)}
                  {m.englishOnly ? " · English only" : ""}
                </span>
              </div>
              <div className="text-xs text-neutral-400">
                {m.speedHint} · {m.qualityHint}
              </div>
            </div>
            <div className="flex items-center gap-2">
              {m.installed && isActive(m) && (
                <span className="rounded-full bg-emerald-500/20 px-2.5 py-1 text-xs text-emerald-300">
                  Active
                </span>
              )}
              {m.installed && !isActive(m) && (
                <>
                  <button
                    onClick={() => activate(m)}
                    className="rounded-md bg-emerald-700 px-3 py-1.5 text-xs font-medium hover:bg-emerald-600"
                  >
                    Use
                  </button>
                  <button
                    onClick={() => deleteModel(m.id).then(refresh)}
                    className="rounded-md bg-neutral-800 px-2 py-1.5 text-xs text-neutral-400 hover:bg-red-900/50 hover:text-red-300"
                  >
                    Delete
                  </button>
                </>
              )}
              {!m.installed && progress[m.id] === undefined && (
                <button
                  onClick={() => {
                    setErrors((e) => {
                      const { [m.id]: _, ...rest } = e;
                      return rest;
                    });
                    setProgress((p) => ({ ...p, [m.id]: 0 }));
                    downloadModel(m.id).catch(() => {});
                  }}
                  className="rounded-md bg-neutral-700 px-3 py-1.5 text-xs font-medium hover:bg-neutral-600"
                >
                  Download
                </button>
              )}
              {progress[m.id] !== undefined && (
                <span className="text-xs text-neutral-400">{progress[m.id]}%</span>
              )}
            </div>
          </div>
          {progress[m.id] !== undefined && (
            <div className="mt-2 h-1.5 w-full overflow-hidden rounded bg-neutral-800">
              <div
                className="h-full bg-emerald-400 transition-all"
                style={{ width: `${progress[m.id]}%` }}
              />
            </div>
          )}
          {errors[m.id] && (
            <div className="mt-2 text-xs text-red-300">{errors[m.id]}</div>
          )}
        </div>
      ))}
      <p className="mt-1 text-xs text-neutral-500">
        Models download from Hugging Face and are checksum-verified. Parakeet is the
        fastest local engine; Whisper large-v3-turbo is the most accurate.
      </p>
    </div>
  );
}
