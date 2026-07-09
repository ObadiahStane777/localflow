import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";

type PillState =
  | "ready"
  | "listening"
  | "locked"
  | "transcribing"
  | "done"
  | "error"
  | "hidden";

function Pill() {
  // Launch flash: confirms the overlay renders without needing a dictation.
  const [state, setState] = useState<PillState>("ready");
  const [detail, setDetail] = useState("");

  useEffect(() => {
    const t = setTimeout(
      () => setState((s) => (s === "ready" ? "hidden" : s)),
      2500,
    );
    const un = listen<{ state: PillState; detail: string }>(
      "pipeline://pill",
      (e) => {
        setState(e.payload.state);
        setDetail(e.payload.detail);
      },
    );
    return () => {
      clearTimeout(t);
      un.then((u) => u());
    };
  }, []);

  if (state === "hidden") return null;

  const label =
    state === "ready"
      ? "LocalFlow ready — hold right ⌘ and speak"
      : state === "listening"
      ? "Listening — tap → to lock"
      : state === "locked"
        ? "Locked on — press hotkey to stop"
        : state === "transcribing"
          ? "Transcribing…"
          : state === "error"
            ? detail || "Something went wrong"
            : detail
              ? detail.length > 42
                ? detail.slice(0, 42) + "…"
                : detail
              : "Done";

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        height: 44,
        width: "fit-content",
        maxWidth: "calc(100vw - 20px)",
        margin: "10px auto",
        padding: "0 18px",
        borderRadius: 22,
        background: "rgba(20, 20, 24, 0.92)",
        border: "1px solid rgba(255,255,255,0.12)",
        boxShadow: "0 4px 24px rgba(0,0,0,0.45)",
        color: "#e5e5e5",
        fontFamily:
          "-apple-system, BlinkMacSystemFont, 'SF Pro Text', sans-serif",
        fontSize: 13,
        fontWeight: 500,
        whiteSpace: "nowrap",
        overflow: "hidden",
      }}
    >
      <span
        style={{
          width: 10,
          height: 10,
          flexShrink: 0,
          borderRadius: "50%",
          background:
            state === "listening" || state === "locked"
              ? "#f87171"
              : state === "transcribing"
                ? "#fbbf24"
                : state === "error"
                  ? "#f87171"
                  : "#34d399",
          animation:
            state === "listening" || state === "locked"
              ? "pulse 1.2s ease-in-out infinite"
              : undefined,
        }}
      />
      {state === "locked" && <span style={{ fontSize: 11 }}>🔒</span>}
      <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{label}</span>
      <style>{`@keyframes pulse { 0%,100% { opacity: 1; transform: scale(1);} 50% { opacity: .4; transform: scale(.8);} }`}</style>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Pill />
  </React.StrictMode>,
);
