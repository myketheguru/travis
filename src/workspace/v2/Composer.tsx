/**
 * Composer — v0.27.2 — voice + text.
 *
 * Always-visible bottom-pinned input. Distinct border + soft glow so
 * the user always knows where to type. Enter submits. Voice button on
 * the left drops a transcript straight in and submits.
 */
import { useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
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
    // v0.28.44 — stash the text so PendingRequestChip can render it
    // on immersive views (map, voice) while the LLM is still working.
    // Cleared automatically after chatBusy flips false.
    useAppStore.getState().setLastSubmittedText(trimmed);
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
        <PendingRequestChip canvasMode={canvasMode} />
        <AttachedDocsStrip />
        <div className="relative">
          <ThinkingGlow visible={isPending} />
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
          className="relative rounded-2xl px-3 py-2 flex items-center gap-2"
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
        </div>
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
        // v0.28.44 — rotating arc instead of static "...". framer's
        // infinite rotate honors prefers-reduced-motion via useReducedMotion
        // in the parent scene; motion.span here inherits the same accessibility
        // context. If the user has reduced motion on, transition duration
        // is what changes, not the visibility of the spinner.
        <motion.span
          animate={{ rotate: 360 }}
          transition={{ duration: 0.9, repeat: Infinity, ease: "linear" }}
          className="inline-flex"
          aria-hidden
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
            <circle
              cx="12"
              cy="12"
              r="9"
              stroke="currentColor"
              strokeOpacity="0.25"
              strokeWidth="2.2"
            />
            <path
              d="M21 12a9 9 0 0 0-9-9"
              stroke="currentColor"
              strokeWidth="2.2"
              strokeLinecap="round"
            />
          </svg>
        </motion.span>
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

/// v0.28.44 — rotating conic-gradient halo behind the composer.
/// Sits underneath the composer surface, sized 4px larger on every
/// edge, blurred so what's visible is the color spilling out around
/// the composer border. When `visible` is false, animates out over
/// 300ms so the composer doesn't jarringly lose its ring the moment
/// a response arrives.
function ThinkingGlow({ visible }: { visible: boolean }) {
  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          key="glow"
          aria-hidden
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
          className="absolute pointer-events-none rounded-[20px] overflow-hidden"
          style={{ inset: -4 }}
        >
          <motion.div
            className="absolute inset-0"
            animate={{ rotate: 360 }}
            transition={{ duration: 2.6, repeat: Infinity, ease: "linear" }}
            style={{
              background:
                "conic-gradient(from 0deg, rgba(189,158,255,0) 0%, rgba(189,158,255,0.75) 15%, rgba(140,230,175,0.55) 35%, rgba(255,210,130,0.55) 55%, rgba(189,158,255,0.75) 80%, rgba(189,158,255,0) 100%)",
              filter: "blur(10px)",
            }}
          />
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/// v0.28.44 — floating chip above the composer that keeps the user's
/// most recent input on screen while Travis is working, but only on
/// immersive views (map/voice/idle) where the chat stream isn't
/// visible. Chat view already shows the user turn in the message
/// list, so the chip would be duplicative there.
///
/// Fades out ~1200ms after chatBusy flips false so the answer has
/// room, then clears lastSubmittedText so the next turn starts fresh.
function PendingRequestChip({ canvasMode }: { canvasMode: string }) {
  const chatBusy = useAppStore((s) => s.chatBusy);
  const activity = useAppStore((s) => s.activity);
  const lastSubmittedText = useAppStore((s) => s.lastSubmittedText);
  const setLastSubmittedText = useAppStore((s) => s.setLastSubmittedText);

  // Only show on immersive views. Chat already surfaces the turn.
  const eligibleView = canvasMode !== "chat";
  // Keep chip up while a turn is in flight, plus a short trailing
  // dwell after it completes so the user sees closure.
  const shouldShow = Boolean(
    eligibleView && lastSubmittedText && (chatBusy || activity === "thinking"),
  );

  useEffect(() => {
    if (!lastSubmittedText) return;
    if (chatBusy || activity === "thinking") return;
    // Turn finished — dwell briefly, then clear.
    const t = window.setTimeout(() => setLastSubmittedText(null), 1200);
    return () => window.clearTimeout(t);
  }, [chatBusy, activity, lastSubmittedText, setLastSubmittedText]);

  const status = statusLabel(canvasMode, activity);

  return (
    <AnimatePresence>
      {shouldShow && (
        <motion.div
          key="pending-chip"
          initial={{ opacity: 0, y: 12, scale: 0.96 }}
          animate={{ opacity: 1, y: 0, scale: 1 }}
          exit={{ opacity: 0, y: 12, scale: 0.96 }}
          transition={{ duration: 0.28, ease: [0.22, 1, 0.36, 1] }}
          className="mb-3 rounded-2xl px-3 py-2 flex items-start gap-2.5"
          style={{
            background:
              "linear-gradient(180deg, rgba(28, 24, 40, 0.78), rgba(20, 18, 30, 0.72))",
            border: "1px solid rgba(189, 158, 255, 0.32)",
            backdropFilter: "blur(14px) saturate(1.2)",
            boxShadow: "0 12px 40px -14px rgba(0, 0, 0, 0.6)",
          }}
        >
          <motion.span
            animate={{ rotate: 360 }}
            transition={{ duration: 1.1, repeat: Infinity, ease: "linear" }}
            className="shrink-0 mt-0.5 inline-flex"
            aria-hidden
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
              <circle cx="12" cy="12" r="9" stroke="rgba(189,158,255,0.28)" strokeWidth="2.2" />
              <path
                d="M21 12a9 9 0 0 0-9-9"
                stroke="rgba(189,158,255,0.95)"
                strokeWidth="2.2"
                strokeLinecap="round"
              />
            </svg>
          </motion.span>
          <div className="min-w-0 flex-1">
            <div
              className="text-[10px] uppercase tracking-[0.22em] font-mono mb-0.5"
              style={{ color: "rgba(189, 158, 255, 0.85)" }}
            >
              {status}
            </div>
            <div
              className="text-[13.5px] leading-snug break-words"
              style={{ color: "rgba(236, 236, 241, 0.94)" }}
            >
              {lastSubmittedText}
            </div>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/// Derives a small status verb from canvas mode + activity so the
/// chip reads as "consulting maps…" on the map view rather than
/// a generic "thinking…". This is a client-side derivation only —
/// no new state to keep in sync with the LLM pipeline.
function statusLabel(canvasMode: string, activity: string): string {
  if (activity === "listening") return "// listening";
  if (activity === "speaking") return "// speaking";
  if (canvasMode === "map") return "// consulting maps";
  if (canvasMode === "voice") return "// on the call";
  return "// on it";
}
