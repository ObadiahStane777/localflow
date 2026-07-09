import { useCallback, useEffect, useState } from "react";
import { snippetAdd, snippetDelete, snippetsList, type SnippetRow } from "../lib/ipc";

export default function Snippets() {
  const [rows, setRows] = useState<SnippetRow[]>([]);
  const [trigger, setTrigger] = useState("");
  const [expansion, setExpansion] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    snippetsList().then(setRows).catch(console.error);
  }, []);

  useEffect(refresh, [refresh]);

  const add = async () => {
    if (!trigger.trim() || !expansion.trim()) return;
    try {
      await snippetAdd(trigger, expansion);
      setTrigger("");
      setExpansion("");
      setError(null);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-neutral-400">
        Say the trigger phrase while dictating and it expands to the full text —
        links, addresses, boilerplate.
      </p>
      <div className="flex flex-col gap-2 rounded-lg bg-neutral-900 p-3">
        <input
          value={trigger}
          onChange={(e) => setTrigger(e.target.value)}
          placeholder='Trigger phrase, e.g. "my calendly"'
          className="rounded-md bg-neutral-800 px-3 py-2 text-sm outline-none placeholder:text-neutral-600 focus:ring-1 focus:ring-emerald-500"
        />
        <textarea
          value={expansion}
          onChange={(e) => setExpansion(e.target.value)}
          placeholder="Expands to, e.g. https://calendly.com/me/30min"
          rows={2}
          className="resize-none rounded-md bg-neutral-800 px-3 py-2 text-sm outline-none placeholder:text-neutral-600 focus:ring-1 focus:ring-emerald-500"
        />
        <button
          onClick={add}
          disabled={!trigger.trim() || !expansion.trim()}
          className="self-end rounded-md bg-emerald-700 px-4 py-1.5 text-xs font-medium hover:bg-emerald-600 disabled:opacity-40"
        >
          Add
        </button>
        {error && <p className="text-xs text-red-300">{error}</p>}
      </div>
      <div className="flex flex-col gap-1.5">
        {rows.map((s) => (
          <div
            key={s.id}
            className="group flex items-center justify-between gap-3 rounded-lg bg-neutral-900 px-4 py-2.5"
          >
            <div className="min-w-0 text-sm">
              <span className="font-medium">"{s.trigger}"</span>
              <span className="ml-2 break-all text-xs text-neutral-500">
                → {s.expansion}
              </span>
            </div>
            <button
              onClick={() => snippetDelete(s.id).then(refresh)}
              className="shrink-0 text-xs text-neutral-600 opacity-0 transition group-hover:opacity-100 hover:text-red-300"
            >
              delete
            </button>
          </div>
        ))}
        {rows.length === 0 && (
          <p className="py-6 text-center text-sm text-neutral-600">No snippets yet.</p>
        )}
      </div>
    </div>
  );
}
