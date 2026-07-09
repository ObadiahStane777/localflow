import { useCallback, useEffect, useState } from "react";
import {
  historyClear,
  historyDelete,
  historyList,
  type HistoryRow,
} from "../lib/ipc";

export default function History({ refreshKey }: { refreshKey: number }) {
  const [rows, setRows] = useState<HistoryRow[]>([]);
  const [query, setQuery] = useState("");
  const [copied, setCopied] = useState<number | null>(null);

  const refresh = useCallback(() => {
    historyList(query || undefined, 200).then(setRows).catch(console.error);
  }, [query]);

  useEffect(refresh, [refresh, refreshKey]);

  const copy = async (row: HistoryRow) => {
    await navigator.clipboard.writeText(row.formatted || row.raw);
    setCopied(row.id);
    setTimeout(() => setCopied(null), 1200);
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search dictations…"
          className="flex-1 rounded-md bg-neutral-900 px-3 py-2 text-sm outline-none placeholder:text-neutral-600 focus:ring-1 focus:ring-emerald-500"
        />
        {rows.length > 0 && (
          <button
            onClick={() => historyClear().then(refresh)}
            className="rounded-md bg-neutral-800 px-3 py-2 text-xs text-neutral-400 hover:bg-red-900/50 hover:text-red-300"
          >
            Clear all
          </button>
        )}
      </div>
      {rows.length === 0 && (
        <p className="py-8 text-center text-sm text-neutral-600">
          No dictations yet.
        </p>
      )}
      <div className="flex flex-col gap-2">
        {rows.map((r) => (
          <div key={r.id} className="group rounded-lg bg-neutral-900 px-4 py-3">
            <p
              className="cursor-pointer select-text whitespace-pre-wrap text-sm"
              onClick={() => copy(r)}
              title="Click to copy"
            >
              {r.formatted || r.raw}
            </p>
            <div className="mt-1 flex items-center justify-between text-xs text-neutral-500">
              <span>
                {new Date(r.ts * 1000).toLocaleString()} · {r.sttProvider} ·{" "}
                {r.latencyMs} ms{copied === r.id ? " · copied ✓" : ""}
              </span>
              <button
                onClick={() => historyDelete(r.id).then(refresh)}
                className="opacity-0 transition group-hover:opacity-100 hover:text-red-300"
              >
                delete
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
