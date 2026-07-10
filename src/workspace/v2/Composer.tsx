/**
 * Composer — v0.27.2 — voice + text.
 *
 * Always-visible bottom-pinned input. Distinct border + soft glow so
 * the user always knows where to type. Enter submits. Voice button on
 * the left drops a transcript straight in and submits.
 */
import { useEffect, useRef, useState } from "react";
import { motion } from "framer-motion";
import { useAppStore } from "../../stores/app";
import { VoiceArmButton } from "./VoiceArmButton";
import { MicMeter } from "./MicMeter";
import { useCanvasMode } from "./canvas/useCanvasMode";

export function Composer() {
  const [text, setText] = useState("");
  const [isFocused, setIsFocused] = useState(false);
  const inputRef = useRef<HTMLTextAreaElement | null>(null);

  const canvasMode = useCanvasMode();
  const focusedThread = useAppStore((s) => s.focusedThread);
  const activity = useAppStore((s) => s.activity);
  const setPendingComposerSubmit = useAppStore(
    (s) => s.setPendingComposerSubmit,
  );
  const noteUserActivity = useAppStore((s) => s.noteUserActivity);

  useEffect(() => {
    inputRef.current?.focus();
  }, [canvasMode]);

  useEffect(() => {
    const el = inputRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
  }, [text]);

  function handleSubmit(overrideText?: string) {
    const trimmed = (overrideText ?? text).trim();
    if (!trimmed) return;
    // v0.28.25 — typed turns explicitly opt out of TTS. Voice paths
    // flip this on before their own setPendingComposerSubmit.
    useAppStore.getState().setSpeakNextResponse(false);
    setPendingComposerSubmit(trimmed);
    setText("");
    noteUserActivity();
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
      e.preventDefault();
      handleSubmit();
    }
  }

  const chatBusy = useAppStore((s) => s.chatBusy);
  const placeholder = placeholderFor(canvasMode, focusedThread, activity);
  // v0.28.25 — decouple from `activity` alone. Voice pipeline touches
  // activity throughout its lifecycle; using chatBusy as the primary
  // gate keeps the composer disabled through the full LLM round-trip
  // and stops the double-submit race the user hit while in map view.
  const isPending = chatBusy || activity === "thinking";

  return (
    <div className="absolute bottom-0 left-0 right-0 z-30 pointer-events-none px-4 pb-4">
      <div className="max-w-3xl mx-auto pointer-events-auto">
        <AttachedDocsStrip />
        <motion.div
          layout
          animate={{
            boxShadow: isFocused
              ? "0 0 40px -8px rgba(124, 92, 255, 0.45), 0 6px 32px -12px rgba(0,0,0,0.6)"
              : "0 6px 32px -12px rgba(0,0,0,0.5)",
            borderColor: isFocused
              ? "rgba(189, 158, 255, 0.55)"
              : "rgba(255, 255, 255, 0.16)",
          }}
          transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
          className="rounded-2xl px-3 py-2 flex items-center gap-2"
          style={{
            background: "rgba(12, 12, 16, 0.85)",
            border: "1px solid rgba(255, 255, 255, 0.16)",
            backdropFilter: "blur(14px)",
          }}
        >
          <VoiceArmButton disabled={isPending} />
          <AttachButton disabled={isPending} />
          {/* v0.27.5 — live level meter appears while the mic is
              armed. Gives an at-a-glance answer to 'is my mic
              actually picking anything up?' before we transcribe. */}
          {(activity === "listening" || activity === "speaking") && (
            <div className="shrink-0 flex items-center">
              <MicMeter />
            </div>
          )}
          <textarea
            ref={inputRef}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={onKeyDown}
            onFocus={() => setIsFocused(true)}
            onBlur={() => setIsFocused(false)}
            placeholder={placeholder}
            disabled={isPending}
            rows={1}
            className="flex-1 resize-none bg-transparent text-[14.5px] focus:outline-none placeholder:text-white/35 leading-relaxed disabled:opacity-50 px-1"
            style={{
              color: "rgba(236, 236, 241, 0.98)",
              minHeight: 24,
              maxHeight: 160,
            }}
          />
          <SubmitButton
            enabled={text.trim().length > 0 && !isPending}
            pending={isPending}
            onClick={() => handleSubmit()}
          />
        </motion.div>
        {focusedThread && (
          <div
            className="mt-2 text-center text-[10px] uppercase tracking-wider font-mono"
            style={{ color: "rgba(189, 158, 255, 0.75)" }}
          >
            replying inside · {focusedThread.title} · esc to leave
          </div>
        )}
      </div>
    </div>
  );
}

function SubmitButton({
  enabled,
  pending,
  onClick,
}: {
  enabled: boolean;
  pending: boolean;
  onClick: () => void;
}) {
  return (
    <motion.button
      whileHover={enabled ? { scale: 1.05 } : {}}
      whileTap={enabled ? { scale: 0.95 } : {}}
      onClick={onClick}
      disabled={!enabled}
      className="shrink-0 w-9 h-9 rounded-xl flex items-center justify-center transition-colors disabled:cursor-not-allowed"
      style={{
        background: enabled
          ? "rgba(189, 158, 255, 0.20)"
          : "rgba(255, 255, 255, 0.04)",
        border: `1px solid ${
          enabled ? "rgba(189, 158, 255, 0.55)" : "rgba(255, 255, 255, 0.08)"
        }`,
        color: enabled ? "rgb(189, 158, 255)" : "rgba(236, 236, 241, 0.35)",
      }}
      aria-label={pending ? "Travis is thinking" : "Send"}
      title={pending ? "Travis is thinking…" : "Send (Enter)"}
    >
      {pending ? (
        <span className="text-[10px] font-mono">...</span>
      ) : (
        <svg
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.8"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
        >
          <path d="M5 12h14M12 5l7 7-7 7" />
        </svg>
      )}
    </motion.button>
  );
}

/// v0.28.25 — paperclip button. Dispatches `travis:pick-and-attach`;
/// AskTab (invisibly mounted) opens the file picker and ingests.
function AttachButton({ disabled }: { disabled: boolean }) {
  return (
    <button
      onClick={() => {
        window.dispatchEvent(new CustomEvent("travis:pick-and-attach"));
      }}
      disabled={disabled}
      className="shrink-0 w-9 h-9 rounded-xl flex items-center justify-center transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
      style={{
        background: "rgba(255, 255, 255, 0.04)",
        border: "1px solid rgba(255, 255, 255, 0.10)",
        color: "rgba(236, 236, 241, 0.7)",
      }}
      aria-label="Attach a file"
      title="Attach a file"
    >
      <svg
        width="16"
        height="16"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66L9.41 17.42a2 2 0 0 1-2.83-2.83l8.49-8.49" />
      </svg>
    </button>
  );
}

/// v0.28.25 — chip strip that mirrors AskTab's attachedDocs from the
/// store. Each chip has a small X that dispatches `travis:remove-attach`.
function AttachedDocsStrip() {
  const docs = useAppStore((s) => s.attachedDocsMirror);
  if (docs.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-1.5 mb-2 pl-1">
      {docs.map((d, i) => {
        const key = d.id != null ? `d-${d.id}` : `p-${d.tempId ?? i}`;
        return (
          <div
            key={key}
            className="flex items-center gap-1.5 px-2 py-1 rounded-lg text-[11.5px]"
            style={{
              background: d.pending
                ? "rgba(255, 210, 130, 0.10)"
                : "rgba(189, 158, 255, 0.10)",
              border: `1px solid ${
                d.pending ? "rgba(255, 210, 130, 0.35)" : "rgba(189, 158, 255, 0.35)"
              }`,
              color: "rgba(236, 236, 241, 0.85)",
              maxWidth: 220,
            }}
          >
            {d.pending && (
              <span
                className="w-1.5 h-1.5 rounded-full"
                style={{
                  background: "rgba(255, 210, 130, 0.9)",
                  boxShadow: "0 0 6px rgba(255, 210, 130, 0.7)",
                }}
                aria-label="ingesting"
              />
            )}
            <span className="truncate">{d.name}</span>
            <button
              onClick={() => {
                window.dispatchEvent(
                  new CustomEvent("travis:remove-attach", {
                    detail: { id: d.id ?? undefined, tempId: d.tempId ?? undefined },
                  }),
                );
              }}
              className="shrink-0 opacity-60 hover:opacity-100"
              aria-label={`Remove ${d.name}`}
              title="Remove"
            >
              <svg
                width="10"
                height="10"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2.5"
                strokeLinecap="round"
                aria-hidden
              >
                <path d="M6 6l12 12M18 6L6 18" />
              </svg>
            </button>
          </div>
        );
      })}
    </div>
  );
}

function placeholderFor(
  canvasMode: string,
  focusedThread: { title: string } | null,
  activity: string,
): string {
  if (focusedThread) return `Continue ${focusedThread.title}…`;
  if (activity === "thinking") return "Travis is thinking…";
  if (canvasMode === "voice") return "Or type instead…";
  if (canvasMode === "map") return "Refine, add a stop, or ask about the route…";
  return "Ask Travis anything…";
}
