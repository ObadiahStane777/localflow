import { useCallback, useEffect, useState } from "react";
import { dictAdd, dictDelete, dictList, type DictEntry } from "../lib/ipc";

export default function Dictionary() {
  const [entries, setEntries] = useState<DictEntry[]>([]);
  const [term, setTerm] = useState("");
  const [variants, setVariants] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    dictList().then(setEntries).catch(console.error);
  }, []);

  useEffect(refresh, [refresh]);

  const add = async () => {
    if (!term.trim()) return;
    try {
      await dictAdd(term, variants);
      setTerm("");
      setVariants("");
      setError(null);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-neutral-400">
        Words the engine keeps getting wrong — names, jargon, brands. The exact
        spelling below wins: it biases transcription and corrects the output.
      </p>
      <div className="flex flex-col gap-2 rounded-lg bg-neutral-900 p-3">
        <input
          value={term}
          onChange={(e) => setTerm(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          placeholder="Correct spelling, e.g. Kubernetes"
          className="rounded-md bg-neutral-800 px-3 py-2 text-sm outline-none placeholder:text-neutral-600 focus:ring-1 focus:ring-emerald-500"
        />
        <input
          value={variants}
          onChange={(e) => setVariants(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          placeholder="Misheard as (optional, comma-separated), e.g. cooper netties"
          className="rounded-md bg-neutral-800 px-3 py-2 text-sm outline-none placeholder:text-neutral-600 focus:ring-1 focus:ring-emerald-500"
        />
        <button
          onClick={add}
          disabled={!term.trim()}
          className="self-end rounded-md bg-emerald-700 px-4 py-1.5 text-xs font-medium hover:bg-emerald-600 disabled:opacity-40"
        >
          Add
        </button>
        {error && <p className="text-xs text-red-300">{error}</p>}
      </div>
      <div className="flex flex-col gap-1.5">
        {entries.map((e) => (
          <div
            key={e.id}
            className="group flex items-center justify-between rounded-lg bg-neutral-900 px-4 py-2.5"
          >
            <div className="text-sm">
              {e.term}
              {e.spokenVariants && (
                <span className="ml-2 text-xs text-neutral-500">
                  ← {e.spokenVariants}
                </span>
              )}
            </div>
            <button
              onClick={() => dictDelete(e.id).then(refresh)}
              className="text-xs text-neutral-600 opacity-0 transition group-hover:opacity-100 hover:text-red-300"
            >
              delete
            </button>
          </div>
        ))}
        {entries.length === 0 && (
          <p className="py-6 text-center text-sm text-neutral-600">
            No entries yet.
          </p>
        )}
      </div>
    </div>
  );
}
