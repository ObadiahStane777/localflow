import { useCallback, useEffect, useState } from "react";
import { styleUpdate, stylesList, type StyleProfileRow } from "../lib/ipc";

export default function Styles() {
  const [rows, setRows] = useState<StyleProfileRow[]>([]);
  const [editing, setEditing] = useState<StyleProfileRow | null>(null);
  const [savedId, setSavedId] = useState<number | null>(null);

  const refresh = useCallback(() => {
    stylesList().then(setRows).catch(console.error);
  }, []);

  useEffect(refresh, [refresh]);

  const save = async () => {
    if (!editing) return;
    await styleUpdate(editing.id, editing.matchPattern, editing.instructions);
    setSavedId(editing.id);
    setTimeout(() => setSavedId(null), 1200);
    setEditing(null);
    refresh();
  };

  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-neutral-400">
        The formatter adapts tone to the app you're dictating into, matched by
        bundle id. Verbatim profiles (code, terminal) skip AI rewriting entirely.
      </p>
      {rows.map((s) => (
        <div key={s.id} className="rounded-lg bg-neutral-900 px-4 py-3">
          <div className="flex items-center justify-between">
            <div className="text-sm font-medium">
              {s.name}
              {s.verbatim && (
                <span className="ml-2 rounded bg-neutral-800 px-1.5 py-0.5 text-[10px] text-neutral-400">
                  verbatim
                </span>
              )}
              {savedId === s.id && (
                <span className="ml-2 text-xs text-emerald-300">saved ✓</span>
              )}
            </div>
            <button
              onClick={() => setEditing(editing?.id === s.id ? null : { ...s })}
              className="text-xs text-neutral-400 hover:text-white"
            >
              {editing?.id === s.id ? "cancel" : "edit"}
            </button>
          </div>
          {editing?.id !== s.id && (
            <>
              <p className="mt-1 text-xs text-neutral-400">{s.instructions}</p>
              <p className="mt-1 break-all text-[10px] text-neutral-600">
                {s.matchPattern}
              </p>
            </>
          )}
          {editing?.id === s.id && (
            <div className="mt-2 flex flex-col gap-2">
              <label className="text-[10px] uppercase tracking-wide text-neutral-500">
                Tone instructions
              </label>
              <textarea
                value={editing.instructions}
                onChange={(e) =>
                  setEditing({ ...editing, instructions: e.target.value })
                }
                rows={3}
                className="resize-none rounded-md bg-neutral-800 px-3 py-2 text-sm outline-none focus:ring-1 focus:ring-emerald-500"
              />
              <label className="text-[10px] uppercase tracking-wide text-neutral-500">
                App bundle ids (comma-separated, * = fallback)
              </label>
              <textarea
                value={editing.matchPattern}
                onChange={(e) =>
                  setEditing({ ...editing, matchPattern: e.target.value })
                }
                rows={2}
                className="resize-none rounded-md bg-neutral-800 px-3 py-2 text-xs outline-none focus:ring-1 focus:ring-emerald-500"
              />
              <button
                onClick={save}
                className="self-end rounded-md bg-emerald-700 px-4 py-1.5 text-xs font-medium hover:bg-emerald-600"
              >
                Save
              </button>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
