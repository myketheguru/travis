import { useCallback, useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PresenceOrb } from "../components/PresenceOrb";
import { ChatReplyBody } from "./ChatReplyBody";
import { EntityChipWithRecall } from "./EntityChipWithRecall";
import { useAppStore } from "../stores/app";
import { hideOverlay } from "../lib/overlay";
import {
  listTasks,
  setTaskStatus,
  type Task,
  type TaskStatus,
} from "../lib/domain";
import {
  journalIngest,
  type JournalIngestResult,
  type MentionChip,
} from "../lib/journal";
import {
  activeConversation,
  resolveConversation,
  type ConversationMessage,
} from "../lib/conversation";
import {
  actionDetails,
  actionHasTechnicalDetails,
  actionLabel,
  actionTechnicalDetails,
  confirmAction,
  declineAction,
  listProposedActions,
  type ProposedAction,
} from "../lib/actions";

type Tab = "open" | "done";

export default function Overlay() {
  const [input, setInput] = useState("");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [tab, setTab] = useState<Tab>("open");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [errorToast, setErrorToast] = useState<string | null>(null);
  const [questions, setQuestions] = useState<string[]>([]);
  const [chatReply, setChatReply] = useState<string | null>(null);
  const [conversationId, setConversationId] = useState<number | null>(null);
  const [history, setHistory] = useState<ConversationMessage[]>([]);
  const [qReply, setQReply] = useState("");
  const [actions, setActions] = useState<ProposedAction[]>([]);
  /// Pre-existing entities the latest extraction recognised — surfaced
  /// as faint chips beneath the chat reply. Cleared at the start of
  /// each capture and replaced when the response arrives.
  const [mentionChips, setMentionChips] = useState<MentionChip[]>([]);
  const qReplyRef = useRef<HTMLInputElement>(null);

  const refreshActions = useCallback(async (cid: number | null) => {
    if (!cid) {
      setActions([]);
      return;
    }
    try {
      const list = await listProposedActions({
        conversationId: cid,
        status: "proposed",
      });
      setActions(list);
    } catch {
      setActions([]);
    }
  }, []);
  const pulse = useAppStore((s) => s.pulse);
  const setActivity = useAppStore((s) => s.setActivity);

  const refresh = useCallback(async (which: TaskStatus) => {
    try {
      const t = await listTasks({ status: which });
      setTasks(t.slice(0, 8));
    } catch {
      setTasks([]);
    }
  }, []);

  useEffect(() => {
    refresh(tab);
  }, [refresh, tab]);

  // Resume any active thread on mount, and re-sync whenever the overlay
  // window regains focus (so picking up the convo in the Ask tab and then
  // reopening this overlay won't show a stale "wondering" card).
  useEffect(() => {
    const refetch = async () => {
      try {
        const thread = await activeConversation();
        if (!thread) {
          setConversationId(null);
          setHistory([]);
          setQuestions([]);
          setChatReply(null);
          setActions([]);
          return;
        }
        setConversationId(thread.conversation.id);
        setHistory(thread.messages);
        const lastAssistant = [...thread.messages]
          .reverse()
          .find((m) => m.role === "assistant");
        let nextQuestions: string[] = [];
        let nextReply: string | null = null;
        if (lastAssistant) {
          nextReply = lastAssistant.content;
          if (lastAssistant.payloadJson) {
            try {
              const payload = JSON.parse(lastAssistant.payloadJson);
              const ext = payload?.extraction;
              if (ext?.clarifyingQuestions?.length > 0) {
                nextQuestions = ext.clarifyingQuestions.slice(0, 2);
              }
            } catch {
              /* ignore malformed payload */
            }
          }
        }
        setQuestions(nextQuestions);
        setChatReply(nextReply);
        refreshActions(thread.conversation.id);
      } catch {
        /* ignore */
      }
    };

    refetch();
    window.addEventListener("focus", refetch);
    return () => window.removeEventListener("focus", refetch);
  }, [refreshActions]);

  // Auto-focus the wondering-card reply when it appears so the user can
  // start typing without an extra click. Using a ref instead of `autoFocus`
  // because the card mounts via AnimatePresence and `autoFocus` only fires
  // once per element instance.
  useEffect(() => {
    if (questions.length > 0) {
      // Wait one frame so the AnimatePresence height transition has begun
      // and the input is in the layout.
      const id = requestAnimationFrame(() => {
        qReplyRef.current?.focus();
      });
      return () => cancelAnimationFrame(id);
    }
  }, [questions.length]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        hideOverlay();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    if (!toast && !errorToast) return;
    const id = setTimeout(() => {
      setToast(null);
      setErrorToast(null);
    }, 5500);
    return () => clearTimeout(id);
  }, [toast, errorToast]);

  const summarize = (r: JournalIngestResult): string => {
    const parts: string[] = [];
    if (r.tasksCompleted.length > 0) {
      parts.push(
        `closed ${r.tasksCompleted.length} task${r.tasksCompleted.length === 1 ? "" : "s"}`,
      );
    }
    if (r.tasksCreated.length > 0) {
      parts.push(`${r.tasksCreated.length} new`);
    }
    const entities = [
      ...(r.entities.coaches ?? []).map((n) => `Coach ${n}`),
      ...(r.entities.schools ?? []),
      ...(r.entities.depts ?? []),
    ];
    if (entities.length > 0) {
      const list = entities.slice(0, 2).join(" · ") + (entities.length > 2 ? `…` : "");
      parts.push(`noted ${list}`);
    }
    if (r.reminders.length > 0) {
      parts.push(`${r.reminders.length} reminder${r.reminders.length === 1 ? "" : "s"}`);
    }
    if (r.capabilityGaps.length > 0) {
      parts.push(`${r.capabilityGaps.length} ask${r.capabilityGaps.length === 1 ? "" : "s"} for me`);
    }
    if (parts.length === 0) return "captured";
    const head = r.routing?.routed
      ? `Captured to ${r.routing.workspaceName}`
      : "Captured";
    return head + " · " + parts.join(" · ");
  };

  const submitText = async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed || busy) return;
    setBusy(true);
    setActivity("thinking");
    setErrorToast(null);
    setToast(null);
    // NOTE: don't clear questions/chatReply here. We let the previous card
    // stay visible across the round-trip and replace its content only when
    // the new response arrives — keeps the conversation card stable.
    setProgress("Saved · thinking…");
    try {
      const r = await journalIngest(trimmed, conversationId ?? undefined);
      setProgress(null);
      await refresh(tab);
      setConversationId(r.conversationId);
      setHistory(r.thread.messages);
      if (r.intent === "conversational" && r.response) {
        setChatReply(r.response);
      } else if (r.extractionOk) {
        setToast(summarize(r));
        if (r.clarifyingQuestions.length === 0) {
          setChatReply(null);
        }
      } else {
        setToast(`Captured (fallback) · ${r.error ?? "extraction failed"}`);
      }
      if (r.clarifyingQuestions.length > 0) {
        setQuestions(r.clarifyingQuestions.slice(0, 2));
      } else {
        // Travis no longer wondering — clear the card.
        setQuestions([]);
      }
      setActions(r.proposedActions);
      setMentionChips(r.mentionChips ?? []);
      if (r.thread.conversation.status === "resolved") {
        setConversationId(null);
      }
    } catch (e) {
      setProgress(null);
      const msg = e instanceof Error ? e.message : String(e);
      const lower = msg.toLowerCase();
      if (lower.includes("api key") || lower.includes("keychain")) {
        setErrorToast("API key missing — open Settings on the splash to set it.");
      } else {
        setErrorToast(msg);
      }
    } finally {
      setActivity("idle");
      setBusy(false);
    }
  };

  const submit = async () => {
    const text = input.trim();
    if (!text) return;
    setInput("");
    await submitText(text);
  };

  const flipTask = async (id: number) => {
    if (tab === "open") {
      await setTaskStatus(id, "done");
    } else {
      await setTaskStatus(id, "open");
    }
    refresh(tab);
  };

  return (
    <main className="h-full w-full flex items-start justify-center pt-8 px-8">
      <motion.div
        initial={{ opacity: 0, y: 8, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
        className="relative w-full max-w-2xl rounded-2xl flex flex-col"
        style={{
          background: "rgba(10, 8, 18, 0.94)",
          backdropFilter: "blur(24px) saturate(140%)",
          WebkitBackdropFilter: "blur(24px) saturate(140%)",
          border: "1px solid rgba(255,255,255,0.07)",
          boxShadow:
            "0 30px 80px -20px rgba(0,0,0,0.75), 0 12px 30px -10px rgba(124,92,255,0.20)",
          maxHeight: "calc(100vh - 64px)",
        }}
      >
        <div
          data-tauri-drag-region
          onMouseDown={(e) => {
            if (e.button !== 0) return;
            const t = e.target as HTMLElement;
            if (t.closest("input, button, a, [data-no-drag]")) return;
            getCurrentWindow().startDragging();
          }}
          className="flex-shrink-0 relative h-9 flex items-center justify-center select-none"
          style={{ cursor: "grab" }}
        >
          <div className="h-1 w-9 rounded-full bg-bone-3/30 pointer-events-none" />
          <div className="absolute right-2.5 top-1.5 opacity-70 pointer-events-none">
            <PresenceOrb size={48} />
          </div>
          {conversationId && (
            <button
              onClick={async () => {
                if (conversationId) {
                  await resolveConversation(conversationId);
                }
                setConversationId(null);
                setHistory([]);
                setQuestions([]);
                setChatReply(null);
              }}
              className="absolute left-3 top-2 text-bone-3 hover:text-bone-2 text-[10px] tracking-wider"
              data-no-drag
              title="End this thread and start fresh"
            >
              · new thread
            </button>
          )}
        </div>

        <div className="flex-1 min-h-0 overflow-y-auto">
        {conversationId && history.length > 1 && (
          <div className="px-7 pt-1 pb-2 text-[10px] text-bone-3 font-mono">
            continuing thread #{conversationId} · {history.length} message{history.length === 1 ? "" : "s"}
          </div>
        )}

        <div
          className="px-7 pt-3 pb-3 relative sticky top-0 z-10"
          style={{ background: "rgba(10, 8, 18, 0.97)" }}
        >
          <input
            autoFocus
            value={input}
            onChange={(e) => {
              pulse();
              setInput(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                submit();
              }
            }}
            placeholder={busy ? "Thinking…" : "What are you thinking?"}
            disabled={busy}
            className="w-full bg-transparent text-bone text-2xl font-light placeholder:text-bone-3/50 focus:outline-none pr-16 disabled:text-bone-2/70"
          />
          <AnimatePresence>
            {busy && (
              <motion.div
                key="bar"
                initial={{ opacity: 0, scaleX: 0.2 }}
                animate={{
                  opacity: 1,
                  scaleX: [0.2, 1, 0.4, 1, 0.6, 1],
                  x: ["-30%", "0%", "30%", "0%", "-30%", "0%"],
                }}
                exit={{ opacity: 0 }}
                transition={{
                  scaleX: { duration: 2.2, repeat: Infinity, ease: "easeInOut" },
                  x: { duration: 2.2, repeat: Infinity, ease: "easeInOut" },
                  opacity: { duration: 0.2 },
                }}
                style={{
                  background:
                    "linear-gradient(90deg, transparent 0%, rgba(190,118,255,0.6) 50%, transparent 100%)",
                  transformOrigin: "center",
                }}
                className="absolute left-7 right-7 bottom-2 h-[1px] rounded-full"
              />
            )}
          </AnimatePresence>
        </div>

        <AnimatePresence>
          {actions.length > 0 && (
            <motion.div
              key="actions"
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
              className="px-7 pb-3 space-y-2"
            >
              {actions.map((a) => (
                <ActionCard
                  key={a.id}
                  action={a}
                  onConfirm={async () => {
                    setActivity("thinking");
                    try {
                      await confirmAction(a.id);
                    } catch (e) {
                      setErrorToast(e instanceof Error ? e.message : String(e));
                    }
                    setActivity("idle");
                    if (conversationId) await refreshActions(conversationId);
                  }}
                  onDecline={async () => {
                    try {
                      await declineAction(a.id);
                    } catch (e) {
                      setErrorToast(e instanceof Error ? e.message : String(e));
                    }
                    if (conversationId) await refreshActions(conversationId);
                  }}
                />
              ))}
            </motion.div>
          )}
        </AnimatePresence>

        <AnimatePresence>
          {chatReply && (
            <motion.div
              key="reply"
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
              className="px-7 pb-3"
            >
              <div className="rounded-xl border border-pulse-2/25 bg-pulse-2/[0.05] p-3 flex items-start gap-3">
                <div className="flex-shrink-0 mt-0.5">
                  <span className="block h-2 w-2 rounded-full bg-pulse-2 shadow-[0_0_8px_rgba(110,196,232,0.7)]" />
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-pulse-2 text-[10px] tracking-[0.18em] uppercase mb-1">
                    Travis
                  </div>
                  <ChatReplyBody
                    reply={chatReply}
                    onSubmit={(text) => submitText(text)}
                    disabled={busy}
                  />
                </div>
                <button
                  onClick={() => setChatReply(null)}
                  className="text-bone-3 text-[10px] hover:text-bone-2 mt-1"
                >
                  dismiss
                </button>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        <AnimatePresence>
          {questions.length > 0 && (
            <motion.div
              key="qs"
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              transition={{ duration: 0.25 }}
              className="px-7 pb-3 relative z-20"
            >
              <div className="rounded-lg border border-pulse/30 bg-pulse/[0.06] p-3">
                <div className="flex items-center justify-between mb-1.5">
                  <span className="text-pulse-2 text-[10px] tracking-[0.18em] uppercase">
                    Travis is wondering
                  </span>
                  <button
                    onClick={() => setQuestions([])}
                    className="text-bone-3 text-[10px] hover:text-bone-2"
                  >
                    dismiss
                  </button>
                </div>
                <ul className="text-bone-2 text-xs leading-relaxed space-y-1">
                  {questions.map((q, i) => (
                    <li key={i}>· {q}</li>
                  ))}
                </ul>
                <div className="mt-3 flex items-center gap-2" data-no-drag>
                  <input
                    ref={qReplyRef}
                    data-no-drag
                    value={qReply}
                    onChange={(e) => {
                      pulse();
                      setQReply(e.target.value);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && !e.shiftKey) {
                        e.preventDefault();
                        const t = qReply.trim();
                        if (!t || busy) return;
                        setQReply("");
                        submitText(t);
                      }
                    }}
                    placeholder={busy ? "Thinking…" : "Reply…"}
                    disabled={busy}
                    className="flex-1 bg-ink-2/40 border border-ink-3/60 focus:border-pulse/60 rounded-md px-2.5 py-1.5 text-bone text-sm placeholder:text-bone-3/50 focus:outline-none transition-colors relative z-20"
                  />
                  <span className="text-bone-3 text-[10px] tracking-wider font-mono opacity-70">
                    ↵ to send
                  </span>
                </div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        {/* Capture chips — pre-existing entities the latest extraction
            recognised. Faint, passive, non-interactive. Rendered just
            above the toast band so they pair with the "Captured" line. */}
        <AnimatePresence>
          {mentionChips.length > 0 && (
            <motion.div
              key="chips"
              initial={{ opacity: 0, y: -2 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -2 }}
              transition={{ duration: 0.25 }}
              className="px-7 pb-2 flex flex-wrap gap-1.5"
            >
              {mentionChips.slice(0, 6).map((c) => (
                <EntityChipWithRecall
                  key={c.entityId}
                  entityId={c.entityId}
                  displayName={c.displayName}
                  kind={c.kind}
                  mentionsCount={c.mentionsCount}
                />
              ))}
            </motion.div>
          )}
        </AnimatePresence>

        <AnimatePresence mode="wait">
          {progress ? (
            <motion.div
              key="progress"
              initial={{ opacity: 0, y: -4 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -4 }}
              transition={{ duration: 0.2 }}
              className="px-7 pb-3 text-pulse-2/90 text-xs flex items-center gap-2"
            >
              <motion.span
                className="h-1.5 w-1.5 rounded-full bg-pulse-2"
                animate={{ opacity: [0.4, 1, 0.4] }}
                transition={{ duration: 1.2, repeat: Infinity }}
              />
              {progress}
            </motion.div>
          ) : errorToast ? (
            <motion.div
              key="err"
              initial={{ opacity: 0, y: -4 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -4 }}
              transition={{ duration: 0.25 }}
              className="px-7 pb-3 text-warn text-xs"
            >
              {errorToast}
            </motion.div>
          ) : toast ? (
            <motion.div
              key="ok"
              initial={{ opacity: 0, y: -4 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -4 }}
              transition={{ duration: 0.25 }}
              className="px-7 pb-3 text-pulse-2 text-xs"
            >
              {toast}
            </motion.div>
          ) : null}
        </AnimatePresence>

        <div className="border-t border-white/[0.04]" />

        <div className="px-5 pt-2 pb-1 flex items-center gap-1">
          <TabBtn active={tab === "open"} onClick={() => setTab("open")}>
            Open
          </TabBtn>
          <TabBtn active={tab === "done"} onClick={() => setTab("done")}>
            Done
          </TabBtn>
        </div>

        <div className="px-3 pb-3 pt-1">
          <AnimatePresence initial={false} mode="popLayout">
            {tasks.length === 0 && (
              <motion.div
                key={`empty-${tab}`}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                className="px-4 py-6 text-bone-3 text-xs"
              >
                {tab === "open"
                  ? "No open tasks. Type and press Enter to capture one."
                  : "Nothing completed yet."}
              </motion.div>
            )}
            {tasks.map((t) => (
              <motion.button
                key={t.id}
                layout
                initial={{ opacity: 0, x: -8 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: 8 }}
                transition={{ duration: 0.2 }}
                onClick={() => flipTask(t.id)}
                title={tab === "open" ? "Mark complete" : "Mark open"}
                className="w-full text-left flex items-center gap-3 px-4 py-2.5 rounded-lg hover:bg-white/[0.03] transition-colors group"
              >
                <span
                  className={
                    "h-4 w-4 rounded-full flex-shrink-0 transition-all " +
                    (tab === "done"
                      ? "bg-pulse-2/60 border border-pulse-2/70"
                      : "border border-bone-3/40 group-hover:border-pulse-2/80 group-hover:bg-pulse-2/10")
                  }
                />
                <span
                  className={
                    "text-sm truncate flex-1 " +
                    (tab === "done"
                      ? "text-bone-3 line-through decoration-bone-3/40"
                      : "text-bone-2")
                  }
                >
                  {t.title}
                </span>
                {tab === "open" && t.dueAt && (
                  <span className="text-bone-3 text-[10px] font-mono">{t.dueAt}</span>
                )}
                {tab === "done" && t.completedAt && (
                  <span className="text-bone-3 text-[10px] font-mono">
                    {t.completedAt.slice(5, 16).replace("T", " ")}
                  </span>
                )}
              </motion.button>
            ))}
          </AnimatePresence>
        </div>
        </div>

        <div className="border-t border-white/[0.04] flex-shrink-0" />
        <div className="flex-shrink-0 px-7 py-3 flex items-center justify-between text-[10px] text-bone-3 font-mono tracking-wider">
          <span>
            <kbd className="px-1.5 py-0.5 rounded bg-white/[0.04] mr-1">ENTER</kbd>
            capture
          </span>
          <span>
            <kbd className="px-1.5 py-0.5 rounded bg-white/[0.04] mr-1">CLICK</kbd>
            {tab === "open" ? "complete" : "reopen"}
          </span>
          <span>
            <kbd className="px-1.5 py-0.5 rounded bg-white/[0.04] mr-1">ESC</kbd>
            dismiss
          </span>
        </div>
      </motion.div>
    </main>
  );
}

function TabBtn({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={
        "relative px-3 py-1.5 text-xs tracking-wider transition-colors " +
        (active ? "text-bone" : "text-bone-3 hover:text-bone-2")
      }
    >
      {children}
      {active && (
        <motion.span
          layoutId="tab-underline"
          className="absolute left-2 right-2 -bottom-[3px] h-[2px] rounded-full bg-pulse"
          transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
        />
      )}
    </button>
  );
}

function ActionCard({
  action,
  onConfirm,
  onDecline,
}: {
  action: ProposedAction;
  onConfirm: () => Promise<void>;
  onDecline: () => Promise<void>;
}) {
  const [busy, setBusy] = useState<"confirm" | "decline" | null>(null);
  const handle = async (kind: "confirm" | "decline") => {
    if (busy) return;
    setBusy(kind);
    try {
      if (kind === "confirm") await onConfirm();
      else await onDecline();
    } finally {
      setBusy(null);
    }
  };
  return (
    <motion.div
      layout
      initial={{ opacity: 0, y: 4 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -4 }}
      transition={{ duration: 0.25 }}
      className={
        action.kind === "run_shell_command"
          ? "rounded-xl border border-warn/30 bg-warn/[0.05] p-3"
          : "rounded-xl border border-pulse/30 bg-pulse/[0.07] p-3"
      }
    >
      <div className="flex items-start gap-3">
        <div className="flex-shrink-0 mt-0.5">
          <span
            className={
              "block h-2 w-2 rounded-full " +
              (action.kind === "run_shell_command"
                ? "bg-warn shadow-[0_0_8px_rgba(255,184,107,0.7)]"
                : "bg-pulse shadow-[0_0_8px_rgba(124,92,255,0.7)]")
            }
          />
        </div>
        <div className="flex-1 min-w-0">
          <div
            className={
              "text-[10px] tracking-[0.18em] uppercase mb-1 " +
              (action.kind === "run_shell_command" ? "text-warn" : "text-pulse-2")
            }
          >
            {actionLabel(action.kind)}
          </div>
          <p className="text-bone-2 text-sm leading-relaxed">
            {action.rationale ?? "(no rationale provided)"}
          </p>
          {actionDetails(action.kind, action.paramsJson) && (
            <p className="text-bone-3 text-[11px] mt-1 font-mono">
              {actionDetails(action.kind, action.paramsJson)}
            </p>
          )}
          {actionHasTechnicalDetails(action.kind) && (
            <details className="mt-2 group">
              <summary className="cursor-pointer text-bone-3 hover:text-bone-2 text-[10px] tracking-wider list-none flex items-center gap-1">
                <span className="transition-transform group-open:rotate-90">›</span>
                <span>
                  {action.kind === "send_email" ? "show full email" : "show command"}
                </span>
              </summary>
              <pre className="mt-1.5 px-2.5 py-1.5 rounded bg-ink/60 border border-ink-3/40 text-bone-2 text-[11px] font-mono whitespace-pre-wrap">
                {actionTechnicalDetails(action.kind, action.paramsJson)}
              </pre>
            </details>
          )}
        </div>
        <div className="flex flex-col gap-1.5 items-end" data-no-drag>
          <button
            onClick={() => handle("confirm")}
            disabled={busy !== null}
            className={
              "px-3 py-1 rounded-full text-[11px] font-medium disabled:opacity-30 transition-colors " +
              (action.kind === "run_shell_command"
                ? "bg-warn/90 text-ink hover:bg-warn"
                : "bg-bone/95 text-ink hover:bg-bone")
            }
          >
            {busy === "confirm"
              ? "Doing…"
              : action.kind === "run_shell_command"
              ? "Allow"
              : "Confirm"}
          </button>
          <button
            onClick={() => handle("decline")}
            disabled={busy !== null}
            className="text-bone-3 hover:text-bone-2 text-[11px] underline-offset-4 hover:underline disabled:opacity-30"
          >
            decline
          </button>
        </div>
      </div>
    </motion.div>
  );
}
