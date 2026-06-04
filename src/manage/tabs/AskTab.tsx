import { useCallback, useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { journalIngest } from "../../lib/journal";
import {
  activeConversation,
  getThread,
  type ConversationMessage,
} from "../../lib/conversation";
import {
  ingestDocument,
  formatBytes,
  type Document,
} from "../../lib/documents";
import { ActiveWorkflowPill } from "../../components/ActiveWorkflowPill";
import { DocumentExtractCard } from "../../overlay/DocumentExtractCard";
import { useAppStore } from "../../stores/app";

export default function AskTab() {
  const [q, setQ] = useState("");
  const [busy, setBusy] = useState(false);
  const [conversationId, setConversationId] = useState<number | null>(null);
  const [messages, setMessages] = useState<ConversationMessage[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [attachedDocs, setAttachedDocs] = useState<Document[]>([]);
  const [expandedDocs, setExpandedDocs] = useState<Set<number>>(new Set());
  const [dropHovering, setDropHovering] = useState(false);
  const setActivity = useAppStore((s) => s.setActivity);
  const pulse = useAppStore((s) => s.pulse);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  // Resume any active thread on mount so questions persist across tab switches.
  useEffect(() => {
    activeConversation()
      .then((thread) => {
        if (!thread) return;
        setConversationId(thread.conversation.id);
        setMessages(thread.messages);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages, busy]);

  const submit = async () => {
    const text = q.trim();
    if (!text && attachedDocs.length === 0) return;
    if (busy) return;
    setBusy(true);
    setError(null);
    setActivity("thinking");

    const docHint =
      attachedDocs.length > 0
        ? "\n\n[Attached: " +
          attachedDocs
            .map((d) => `${d.displayName} (${d.kind}, doc#${d.id})`)
            .join(", ") +
          "]"
        : "";
    const submitPayload = (text || "(attached files for review)") + docHint;

    const optimistic: ConversationMessage = {
      id: -Date.now(),
      conversationId: conversationId ?? -1,
      role: "user",
      content: submitPayload,
      payloadJson: null,
      createdAt: new Date().toISOString(),
    };
    setMessages((prev) => [...prev, optimistic]);
    setQ("");
    setAttachedDocs([]);
    setExpandedDocs(new Set());

    try {
      const r = await journalIngest(
        submitPayload,
        conversationId ?? undefined,
      );
      setConversationId(r.conversationId);
      setMessages(r.thread.messages);
    } catch (e) {
      setMessages((prev) => prev.filter((m) => m.id !== optimistic.id));
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setActivity("idle");
      setBusy(false);
    }
  };

  const ingestFile = useCallback(
    async (filePath: string) => {
      try {
        const doc = await ingestDocument({
          filePath,
          conversationId: conversationId,
        });
        setAttachedDocs((prev) =>
          prev.find((d) => d.id === doc.id) ? prev : [...prev, doc],
        );
      } catch (e) {
        setError(`Couldn't attach ${filePath.split(/[\\/]/).pop()}: ${(e as Error).message ?? e}`);
      }
    },
    [conversationId],
  );

  const handlePickFile = useCallback(async () => {
    try {
      const selected = await openFileDialog({
        multiple: true,
        title: "Attach to Travis",
        filters: [
          {
            name: "Documents",
            extensions: ["pdf", "csv", "xlsx", "xls", "xlsm", "xlsb", "ods", "png", "jpg", "jpeg", "webp"],
          },
        ],
      });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      for (const p of paths) {
        await ingestFile(p);
      }
    } catch (e) {
      setError((e as Error).message ?? String(e));
    }
  }, [ingestFile]);

  // Drag-drop listener on the main window. Mirrors the overlay's
  // wiring so Taylor can drop a file anywhere on the Ask surface.
  useEffect(() => {
    let unlistenEnter: (() => void) | null = null;
    let unlistenDrop: (() => void) | null = null;
    let unlistenLeave: (() => void) | null = null;
    let cancelled = false;
    (async () => {
      try {
        const win = getCurrentWindow();
        unlistenEnter = await win.listen<unknown>(
          "tauri://drag-enter",
          () => {
            if (!cancelled) setDropHovering(true);
          },
        );
        unlistenLeave = await win.listen<unknown>(
          "tauri://drag-leave",
          () => {
            if (!cancelled) setDropHovering(false);
          },
        );
        unlistenDrop = await win.listen<{ paths?: string[] }>(
          "tauri://drag-drop",
          async (event) => {
            if (cancelled) return;
            setDropHovering(false);
            const paths = event?.payload?.paths ?? [];
            for (const p of paths) {
              await ingestFile(p);
            }
          },
        );
      } catch {
        /* drag-drop unsupported; degrade silently */
      }
    })();
    return () => {
      cancelled = true;
      try {
        unlistenEnter?.();
      } catch {
        /* ignore */
      }
      try {
        unlistenDrop?.();
      } catch {
        /* ignore */
      }
      try {
        unlistenLeave?.();
      } catch {
        /* ignore */
      }
    };
  }, [ingestFile]);

  const reset = async () => {
    setConversationId(null);
    setMessages([]);
    setError(null);
    setQ("");
  };

  const reload = async () => {
    if (!conversationId) return;
    try {
      const t = await getThread(conversationId);
      setMessages(t.messages);
    } catch {
      /* ignore */
    }
  };

  // Refresh thread when a different surface (overlay) appends a turn to the same conversation.
  useEffect(() => {
    if (!conversationId) return;
    const id = setInterval(reload, 8000);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [conversationId]);

  const empty = messages.length === 0;

  return (
    <div className="px-10 pt-4 pb-6 max-w-2xl mx-auto flex flex-col h-full">
      {!empty && (
        <div className="flex items-center justify-between text-bone-3 text-[10px] tracking-[0.18em] uppercase font-mono mb-2">
          <span>thread #{conversationId} · {messages.length} message{messages.length === 1 ? "" : "s"}</span>
          <button
            onClick={reset}
            className="hover:text-bone-2 normal-case tracking-wider underline-offset-4 hover:underline"
          >
            new chat
          </button>
        </div>
      )}

      <div
        ref={scrollRef}
        className={
          "flex-1 overflow-y-auto flex flex-col gap-3 " +
          (empty ? "items-center justify-center" : "")
        }
      >
        {empty ? (
          <p className="text-bone-3 text-sm text-center max-w-md leading-relaxed">
            Ask Travis anything, capture an op, or just think out loud. Travis
            will pull from your past notes, open tasks, and what it knows about
            you.
            <br />
            <span className="text-bone-3/70 text-xs">
              Same surface as Cmd+J — works for questions, captures, and follow-ups.
            </span>
          </p>
        ) : (
          <>
            <AnimatePresence initial={false}>
              {messages.map((m) => (
                <MessageBubble key={m.id} message={m} />
              ))}
            </AnimatePresence>
            {busy && (
              <motion.div
                key="thinking"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                className="self-start text-bone-3 text-xs flex items-center gap-2"
              >
                <span className="inline-block h-1.5 w-1.5 rounded-full bg-pulse-2 animate-pulse" />
                thinking…
              </motion.div>
            )}
          </>
        )}
      </div>

      <div
        className="pt-3 mt-3 border-t border-white/[0.04] flex flex-col gap-1"
        style={{
          background: dropHovering ? "rgba(124, 92, 255, 0.08)" : "transparent",
          transition: "background 200ms ease-out",
          borderRadius: dropHovering ? 8 : 0,
          outline: dropHovering ? "1px dashed rgba(124, 92, 255, 0.45)" : "none",
          outlineOffset: -1,
        }}
      >
        <ActiveWorkflowPill conversationId={conversationId} />

        {attachedDocs.length > 0 && (
          <>
            <div className="flex flex-wrap gap-1.5 pb-1.5">
              {attachedDocs.map((d) => {
                const expanded = expandedDocs.has(d.id);
                return (
                  <button
                    key={d.id}
                    onClick={() =>
                      setExpandedDocs((prev) => {
                        const next = new Set(prev);
                        if (next.has(d.id)) next.delete(d.id);
                        else next.add(d.id);
                        return next;
                      })
                    }
                    className={
                      "inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[11px] font-mono transition-colors " +
                      (expanded
                        ? "bg-pulse/25 border border-pulse/50 text-bone"
                        : "bg-pulse/12 border border-pulse/25 text-bone-2 hover:bg-pulse/18")
                    }
                    title={`${d.originalFilename} · ${formatBytes(d.sizeBytes)} · click to ${
                      expanded ? "collapse" : "view extracted fields"
                    }`}
                  >
                    <span className="text-pulse">◈</span>
                    <span className="truncate max-w-[200px]">{d.displayName}</span>
                    <span className="text-bone-3">{formatBytes(d.sizeBytes)}</span>
                    <span
                      onClick={(e) => {
                        e.stopPropagation();
                        setAttachedDocs((prev) =>
                          prev.filter((x) => x.id !== d.id),
                        );
                        setExpandedDocs((prev) => {
                          const next = new Set(prev);
                          next.delete(d.id);
                          return next;
                        });
                      }}
                      className="text-bone-3 hover:text-bone-2 ml-0.5 cursor-pointer"
                      title="Remove from this turn"
                    >
                      ×
                    </span>
                  </button>
                );
              })}
              {dropHovering && (
                <div className="text-[11px] text-pulse-2/80 font-mono self-center">
                  drop to attach…
                </div>
              )}
            </div>
            <AnimatePresence>
              {attachedDocs
                .filter((d) => expandedDocs.has(d.id))
                .map((d) => (
                  <div key={`card-${d.id}`} className="pb-2">
                    <DocumentExtractCard
                      documentId={d.id}
                      onClose={() => {
                        setExpandedDocs((prev) => {
                          const next = new Set(prev);
                          next.delete(d.id);
                          return next;
                        });
                      }}
                    />
                  </div>
                ))}
            </AnimatePresence>
          </>
        )}

        {attachedDocs.length === 0 && dropHovering && (
          <div className="text-[11px] text-pulse-2 font-mono pb-1">
            drop file to attach
          </div>
        )}

        <div className="flex items-center gap-2">
          <button
            onClick={handlePickFile}
            disabled={busy}
            title="Attach file"
            className="shrink-0 h-8 w-8 flex items-center justify-center rounded-full text-bone-3 hover:text-bone-2 hover:bg-white/[0.04] disabled:opacity-50 transition-colors"
          >
            <svg
              viewBox="0 0 24 24"
              width="16"
              height="16"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48" />
            </svg>
          </button>
          <input
            autoFocus
            value={q}
            onChange={(e) => {
              pulse();
              setQ(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                submit();
              }
            }}
            placeholder={
              empty
                ? attachedDocs.length > 0
                  ? "Tell Travis what to do with the file(s)…"
                  : "Type anything…"
                : attachedDocs.length > 0
                  ? "Add a note for the attachment(s)…"
                  : "Continue…"
            }
            disabled={busy}
            className="w-full bg-transparent px-1 py-2 text-bone text-base font-light placeholder:text-bone-3/50 focus:outline-none disabled:text-bone-2/70"
          />
        </div>
        {error && <p className="text-warn text-xs">{error}</p>}
      </div>
    </div>
  );
}

function MessageBubble({ message }: { message: ConversationMessage }) {
  const isUser = message.role === "user";

  // Decode any sources stashed in payload_json on assistant messages so we
  // can show them collapsibly under the bubble.
  let sources: { kind: string; sourceId: number; text: string; createdAt: string }[] = [];
  if (!isUser && message.payloadJson) {
    try {
      const p = JSON.parse(message.payloadJson);
      const list = p?.sources ?? p?.extraction?.memorySources;
      if (Array.isArray(list)) sources = list;
    } catch {
      /* ignore */
    }
  }

  return (
    <motion.div
      initial={{ opacity: 0, y: 4 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2 }}
      className={"flex flex-col gap-1 " + (isUser ? "items-end" : "items-start")}
    >
      <span className="text-[9px] tracking-[0.2em] uppercase text-bone-3/70">
        {isUser ? "you" : "travis"}
      </span>
      <p
        className={
          "text-sm leading-relaxed whitespace-pre-wrap max-w-[85%] " +
          (isUser ? "text-bone" : "text-bone-2")
        }
      >
        {message.content}
      </p>
      {sources.length > 0 && (
        <details className="mt-1">
          <summary className="cursor-pointer text-pulse-2/70 hover:text-pulse-2 text-[10px] tracking-wider list-none flex items-center gap-1">
            <span>›</span>
            <span>{sources.length} source{sources.length === 1 ? "" : "s"}</span>
          </summary>
          <div className="mt-1.5 flex flex-col gap-1">
            {sources.map((s, i) => (
              <div
                key={i}
                className="rounded border border-ink-3/40 bg-ink-2/20 px-2.5 py-1.5"
              >
                <div className="flex items-center gap-2 text-[9px] font-mono text-bone-3 mb-0.5">
                  <span className="text-pulse-2/80">{s.kind}#{s.sourceId}</span>
                  <span className="ml-auto opacity-60">{s.createdAt?.slice(0, 10)}</span>
                </div>
                <p className="text-bone-3 text-[11px] leading-snug line-clamp-3">{s.text}</p>
              </div>
            ))}
          </div>
        </details>
      )}
    </motion.div>
  );
}
