import { useCallback, useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { motion, AnimatePresence } from "framer-motion";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { journalIngest } from "../../lib/journal";
import {
  activeConversation,
  getThread,
  loadMoreMessages,
  deleteMessageAndAfter,
  type ConversationMessage,
} from "../../lib/conversation";
import {
  ingestDocument,
  formatBytes,
  getDocument,
  type Document,
} from "../../lib/documents";
import {
  listSteps,
  subscribeSteps,
  parseRow,
  type ParsedStep,
  type StepEvent,
} from "../../lib/steps";
import { ActiveWorkflowPill } from "../../components/ActiveWorkflowPill";
import { DocumentExtractCard } from "../../overlay/DocumentExtractCard";
import { ChatTurn } from "../../chat/ChatTurn";
import { AutoGrowTextarea } from "../../chat/AutoGrowTextarea";
import { CaseHeaderStrip } from "../../chat/CaseHeaderStrip";
import { ConversationSwitcher } from "../../chat/ConversationSwitcher";
import { ActionCard } from "../../chat/ActionCard";
import {
  confirmAction,
  declineAction,
  listProposedActions,
  type ProposedAction,
} from "../../lib/actions";
import { useScrollAnchor } from "../../chat/useScrollAnchor";
import { useAppStore } from "../../stores/app";

/// Attachment in flight: shown immediately, swapped for a real
/// Document when ingestDocument resolves.
interface PendingAttachment {
  tempId: string;
  filename: string;
  sizeBytes: number;
  kind: "pending";
}

type Attachment = Document | PendingAttachment;

const isPending = (a: Attachment): a is PendingAttachment =>
  (a as PendingAttachment).kind === "pending";

export default function AskTab() {
  const [q, setQ] = useState("");
  const [busy, setBusy] = useState(false);
  const activeConversationId = useAppStore((s) => s.activeConversationId);
  const setActiveConversationId = useAppStore((s) => s.setActiveConversationId);
  const [justReadied, setJustReadied] = useState(false);
  const inputRef = useRef<HTMLTextAreaElement | null>(null);

  // Re-focus the input + pulse the border whenever Travis finishes a
  // response so the user can see clearly that it's their turn.
  useEffect(() => {
    if (!busy) {
      inputRef.current?.focus();
      setJustReadied(true);
      const id = setTimeout(() => setJustReadied(false), 1800);
      return () => clearTimeout(id);
    }
  }, [busy]);
  const [messages, setMessages] = useState<ConversationMessage[]>([]);
  // v0.19.6 — bridge from DocumentsTab. The "attach to chat" button
  // in the library dispatches a window event; we listen here, look
  // up the doc, and add it to attachedDocs as if the user dropped it.
  useEffect(() => {
    const onAttach = async (e: Event) => {
      const detail = (e as CustomEvent).detail as
        | { documentId?: number }
        | undefined;
      const id = detail?.documentId;
      if (typeof id !== "number") return;
      try {
        const doc = await getDocument(id);
        if (!doc) return;
        setAttachedDocs((prev) => {
          // Dedup — don't add if already attached (by id).
          if (prev.some((a) => !isPending(a) && a.id === doc.id)) return prev;
          return [...prev, doc];
        });
      } catch {
        /* ignore */
      }
    };
    window.addEventListener(
      "travis:attach-document-from-library",
      onAttach as EventListener,
    );
    return () => {
      window.removeEventListener(
        "travis:attach-document-from-library",
        onAttach as EventListener,
      );
    };
  }, []);

  // v0.20.0 — surface pending proposed_actions in the chat feed.
  // Loads on conversation change + after each turn (busy → false
  // transition) + on a low-frequency poll while the chat is open.
  // The ActionCard's confirm/decline handlers refetch so the list
  // shrinks immediately.
  const refreshPendingActions = useCallback(async () => {
    if (!activeConversationId) {
      setPendingActions([]);
      return;
    }
    try {
      const list = await listProposedActions({
        conversationId: activeConversationId,
        status: "proposed",
      });
      setPendingActions(list);
    } catch {
      /* ignore — empty list is the safe default */
    }
  }, [activeConversationId]);
  useEffect(() => {
    refreshPendingActions();
  }, [refreshPendingActions]);
  useEffect(() => {
    if (!activeConversationId) return;
    const id = setInterval(refreshPendingActions, 5000);
    return () => clearInterval(id);
  }, [activeConversationId, refreshPendingActions]);

  // v0.18.2 — chunked history. `haveOlder` flips false when a fetch
  // returns nothing (we've hit the start of the conversation).
  // `loadingOlder` debounces concurrent fetches while the user is
  // scrolling fast at the top edge.
  const [haveOlder, setHaveOlder] = useState(true);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [steps, setSteps] = useState<ParsedStep[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [pendingActions, setPendingActions] = useState<ProposedAction[]>([]);
  const [attachedDocs, setAttachedDocs] = useState<Attachment[]>([]);
  const [expandedDocs, setExpandedDocs] = useState<Set<number>>(new Set());
  const [dropHovering, setDropHovering] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<number | null>(null);
  const setActivity = useAppStore((s) => s.setActivity);
  const pulse = useAppStore((s) => s.pulse);

  // Smart scroll anchor: jumps to bottom on first paint; auto-tracks
  // bottom only when the user is already there. If they scroll up, new
  // content does NOT yank them back.
  // v0.18.2 — scroll-near-top loader. When the chat scrolls within
  // 120px of the top AND there's older history available, fetch the
  // next 50 older messages. Preserves scroll position by recording
  // scrollHeight before prepend and restoring scrollTop to the same
  // distance-from-the-old-top after the new rows render.
  const loadOlderIfNeeded = useCallback(
    async (el: HTMLDivElement) => {
      if (!haveOlder || loadingOlder) return;
      if (!activeConversationId) return;
      if (el.scrollTop > 120) return;
      const earliest = messages[0]?.id;
      if (!earliest) return;
      setLoadingOlder(true);
      const heightBefore = el.scrollHeight;
      const topBefore = el.scrollTop;
      try {
        const older = await loadMoreMessages(activeConversationId, earliest);
        if (older.length === 0) {
          setHaveOlder(false);
          return;
        }
        flushSync(() => {
          setMessages((prev) => [...older, ...prev]);
        });
        const heightAfter = el.scrollHeight;
        el.scrollTop = topBefore + (heightAfter - heightBefore);
      } catch {
        /* network/IPC blip — next scroll tick will retry */
      } finally {
        setLoadingOlder(false);
      }
    },
    [haveOlder, loadingOlder, activeConversationId, messages],
  );

  const { ref: scrollRef, atBottom, scrollToBottom } = useScrollAnchor(
    `${messages.length}:${steps.length}:${busy ? "1" : "0"}`,
  );

  // v0.16.0 — split the step lifecycle into two effects so live events
  // stream regardless of when `activeConversationId` arrives.
  //
  // Old bug: subscription was gated on activeConversationId, so during
  // the FIRST turn of a fresh chat (id starts null → backend assigns id
  // mid-call → frontend only learns it after journalIngest returns),
  // every step event the backend emitted was dropped. They only showed
  // up after the chat reloaded from the DB via listSteps. This effect
  // change makes the subscription persistent and uses a ref to filter
  // by the current id without re-subscribing.
  const activeConvIdRef = useRef<number | null>(activeConversationId);
  useEffect(() => {
    activeConvIdRef.current = activeConversationId;
  }, [activeConversationId]);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    subscribeSteps((event: StepEvent) => {
      // Apply against whichever conversation is currently active. If
      // the frontend doesn't know the id yet (first-turn race), only
      // a "started" event carries conversationId — accept it
      // optimistically. Note / result / completed events match by
      // step id, so they don't need the filter to find their parent.
      const fallback =
        event.event === "started" ? event.conversationId : 0;
      const currentId = activeConvIdRef.current ?? fallback;
      setSteps((prev) => applyStepEvent(prev, event, currentId));
    })
      .then((fn) => {
        if (cancelled) {
          try {
            fn();
          } catch {
            /* ignore */
          }
        } else {
          unlisten = fn;
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      try {
        unlisten?.();
      } catch {
        /* ignore */
      }
    };
  }, []);

  // v0.18.2 — reset chunked-history state whenever the active
  // conversation changes. A fresh thread always begins assuming
  // older history might exist.
  useEffect(() => {
    setHaveOlder(true);
    setLoadingOlder(false);
  }, [activeConversationId]);

  useEffect(() => {
    if (!activeConversationId) {
      setSteps([]);
      return;
    }
    let cancelled = false;
    listSteps(activeConversationId)
      .then((dbSteps) => {
        if (cancelled) return;
        // v0.17.1 — merge instead of replace. The listSteps resync
        // races with the live step-event subscription: if any live
        // events arrived between query-start and query-resolve, a
        // raw setSteps(dbSteps) wipes them. Merge by id, prefer the
        // newer-shaped row (the one with `status !== "running"` or
        // more notes), and union the orderings.
        setSteps((prev) =>
          mergeStepLists(prev, dbSteps, activeConversationId),
        );
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [activeConversationId]);

  // v0.17.3 — polling fallback for live step visibility. The
  // subscribeSteps Tauri-event path has been unreliable in
  // production (live events deliver inconsistently during long
  // turns; user reports steps only appearing on remount). Belt-
  // and-suspenders: while `busy` is true, poll `list_steps` every
  // 1.5s and merge with current state. Cheap (a single indexed
  // DB read per tick), guaranteed visibility within ~1.5s.
  // Stops the moment the turn finishes.
  useEffect(() => {
    if (!busy || !activeConversationId) return;
    let cancelled = false;
    const intervalId = setInterval(async () => {
      if (cancelled || !activeConversationId) return;
      try {
        const dbSteps = await listSteps(activeConversationId);
        if (cancelled) return;
        setSteps((prev) =>
          mergeStepLists(prev, dbSteps, activeConversationId),
        );
      } catch {
        /* network/IPC blip — next tick retries */
      }
    }, 1500);
    return () => {
      cancelled = true;
      clearInterval(intervalId);
    };
  }, [busy, activeConversationId]);

  // Resume a thread on mount. Prefer the persisted conversation id so
  // tab-switches always restore the same chat. Fall back to the
  // backend's "awaiting_user" heuristic only on first run.
  const didInitialResume = useRef(false);
  useEffect(() => {
    if (didInitialResume.current) return;
    didInitialResume.current = true;
    let cancelled = false;
    (async () => {
      if (activeConversationId) {
        try {
          const t = await getThread(activeConversationId);
          if (!cancelled) setMessages(t.messages);
          return;
        } catch {
          // Conversation no longer exists — drop the stale id and
          // try the awaiting_user fallback.
          if (!cancelled) setActiveConversationId(null);
        }
      }
      try {
        const thread = await activeConversation();
        if (cancelled || !thread) return;
        setActiveConversationId(thread.conversation.id);
        setMessages(thread.messages);
      } catch {
        /* ignore */
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // v0.18.3 — when the user switches threads via the ConversationSwitcher
  // (activeConversationId changes after the initial resume), reload the
  // messages for the new thread. Null means "new chat" — clear state.
  useEffect(() => {
    if (!didInitialResume.current) return;
    let cancelled = false;
    if (activeConversationId == null) {
      setMessages([]);
      setSteps([]);
      return () => {
        cancelled = true;
      };
    }
    (async () => {
      try {
        const t = await getThread(activeConversationId);
        if (!cancelled) {
          setMessages(t.messages);
        }
      } catch {
        if (!cancelled) {
          setMessages([]);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeConversationId]);

  const submit = async () => {
    const text = q.trim();
    // Strip pending attachments — they aren't ingested yet and can't be
    // referenced by doc#N. The user must wait for the spinner to finish.
    const ingested = attachedDocs.filter((a): a is Document => !isPending(a));
    if (!text && ingested.length === 0) return;
    if (busy) return;
    setBusy(true);
    setError(null);
    setActivity("thinking");

    const docHint =
      ingested.length > 0
        ? "\n\n[Attached: " +
          ingested
            .map((d) => `${d.displayName} (${d.kind}, doc#${d.id})`)
            .join(", ") +
          "]"
        : "";
    const submitPayload = (text || "(attached files for review)") + docHint;

    // Optimistic echo: same stable id used as the React key. When the
    // server thread comes back, we *merge* — keeping this row mounted
    // so AnimatePresence doesn't unmount + remount the user bubble.
    const optimisticId = -Date.now();
    const optimistic: ConversationMessage = {
      id: optimisticId,
      conversationId: activeConversationId ?? -1,
      role: "user",
      content: submitPayload,
      payloadJson: null,
      createdAt: new Date().toISOString(),
    };
    // v0.14.4: flushSync forces a synchronous commit + paint so the
    // user-bubble lands in the DOM BEFORE the busy=true state churn
    // adds the "thinking…" live-turn. Without flushSync, React's
    // batching can coalesce both updates into one render where the
    // live-turn appears immediately, pushing the optimistic above
    // the viewport's smart-scroll fold.
    flushSync(() => {
      setMessages((prev) => [...prev, optimistic]);
      setQ("");
      setAttachedDocs([]);
      setExpandedDocs(new Set());
    });
    // After paint, scroll the new user bubble into view at the top of
    // the visible area so it's clearly anchored even when the live-turn
    // and message together exceed one viewport height.
    requestAnimationFrame(() => {
      const el = scrollRef.current?.querySelector(
        `[data-message-id="${optimisticId}"]`,
      );
      if (el && "scrollIntoView" in el) {
        (el as HTMLElement).scrollIntoView({ block: "start", behavior: "auto" });
      }
    });

    try {
      const r = await journalIngest(
        submitPayload,
        activeConversationId ?? undefined,
      );
      setActiveConversationId(r.conversationId);
      // Merge: replace the optimistic row by content match, keep
      // every earlier row intact, append anything new from the server.
      setMessages((prev) => mergeServerThread(prev, optimisticId, r.thread.messages));
    } catch (e) {
      setMessages((prev) => prev.filter((m) => m.id !== optimisticId));
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setActivity("idle");
      setBusy(false);
    }
  };

  const ingestFile = useCallback(
    async (filePath: string) => {
      const filename = filePath.split(/[\\/]/).pop() ?? filePath;
      const tempId = `pending-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
      // 1. Push placeholder synchronously so the input shows immediate
      //    feedback (no multi-second wait).
      setAttachedDocs((prev) => [
        ...prev,
        { tempId, filename, sizeBytes: 0, kind: "pending" },
      ]);
      try {
        const doc = await ingestDocument({
          filePath,
          conversationId: activeConversationId,
        });
        // 2. Swap placeholder for real doc.
        setAttachedDocs((prev) => {
          const exists = prev.find(
            (a) => !isPending(a) && (a as Document).id === doc.id,
          );
          if (exists) return prev.filter((a) => !isPending(a) || a.tempId !== tempId);
          return prev.map((a) =>
            isPending(a) && a.tempId === tempId ? doc : a,
          );
        });
      } catch (e) {
        setAttachedDocs((prev) =>
          prev.filter((a) => !isPending(a) || a.tempId !== tempId),
        );
        setError(`Couldn't attach ${filename}: ${(e as Error).message ?? e}`);
      }
    },
    [activeConversationId],
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
    setActiveConversationId(null);
    setMessages([]);
    setError(null);
    setQ("");
  };

  const reload = async () => {
    if (!activeConversationId) return;
    try {
      const t = await getThread(activeConversationId);
      setMessages(t.messages);
    } catch {
      /* ignore */
    }
  };

  const handleDeleteRequest = useCallback((messageId: number) => {
    setPendingDelete(messageId);
  }, []);

  const handleDeleteConfirm = useCallback(async () => {
    if (pendingDelete == null || !activeConversationId) return;
    const id = pendingDelete;
    setPendingDelete(null);
    try {
      await deleteMessageAndAfter(activeConversationId, id);
      // Optimistic local trim — server already removed.
      setMessages((prev) => prev.filter((m) => m.id < id));
    } catch (e) {
      setError(`Could not delete message: ${(e as Error).message ?? e}`);
    }
  }, [pendingDelete, activeConversationId]);

  const handleDeleteCancel = useCallback(() => {
    setPendingDelete(null);
  }, []);

  // Refresh thread when a different surface (overlay) appends a turn to the same conversation.
  useEffect(() => {
    if (!activeConversationId) return;
    const id = setInterval(reload, 8000);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeConversationId]);

  const empty = messages.length === 0;
  const deleteCount =
    pendingDelete != null
      ? messages.filter((m) => m.id >= pendingDelete).length
      : 0;

  return (
    <div className="px-10 pt-4 pb-6 max-w-2xl mx-auto flex flex-col h-full">
      {!empty && (
        <div className="flex items-center justify-between text-bone-3 text-[10px] tracking-[0.18em] uppercase font-mono mb-2">
          <span>thread #{activeConversationId} · {messages.length} message{messages.length === 1 ? "" : "s"}</span>
          <button
            onClick={reset}
            className="hover:text-bone-2 normal-case tracking-wider underline-offset-4 hover:underline"
          >
            new chat
          </button>
        </div>
      )}

      {/* v0.18.3 — chat-thread switcher. Sits above the case strip so
          users can jump between conversations or start a fresh one
          from the chat surface itself. */}
      <div className="flex items-center justify-start pb-1">
        <ConversationSwitcher />
      </div>

      {/* v0.16.0 — case substrate. Renders only when this conversation
          is linked to an open case (auto-opened by journal_ingest when
          the workflow + multi-doc + depth triggers fire). */}
      <CaseHeaderStrip
        conversationId={activeConversationId}
        onClose={() => {
          /* nothing to clean up locally; the strip refetches on its
             own interval and hides when the case closes. */
        }}
        onSwitchToConversation={(id) => setActiveConversationId(id)}
      />


      <div className="relative flex-1 min-h-0 flex flex-col">
        <div
          ref={scrollRef}
          onScroll={(e) => loadOlderIfNeeded(e.currentTarget)}
          className={
            "flex-1 min-h-0 overflow-y-auto flex flex-col gap-3 pr-2 -mr-2 " +
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
              {loadingOlder && (
                <div className="text-center text-[10px] tracking-[0.2em] uppercase text-bone-3/70 py-1">
                  Loading earlier messages…
                </div>
              )}
              {!haveOlder && messages.length > 0 && (
                <div className="text-center text-[10px] tracking-[0.2em] uppercase text-bone-3/50 py-1">
                  Start of conversation
                </div>
              )}
              <AnimatePresence initial={false}>
                {renderTurns(
                  messages,
                  steps,
                  busy,
                  pendingDelete,
                  handleDeleteRequest,
                  handleDeleteConfirm,
                  handleDeleteCancel,
                  deleteCount,
                )}
              </AnimatePresence>
            </>
          )}
        </div>

        {!atBottom && !empty && (
          <button
            onClick={() => scrollToBottom("smooth")}
            className="absolute left-1/2 -translate-x-1/2 bottom-2 px-3 py-1.5 rounded-full text-[11px] font-mono tracking-wide shadow-lg flex items-center gap-1.5 transition-opacity"
            style={{
              background: "rgba(20, 22, 30, 0.92)",
              border: "1px solid rgba(124, 92, 255, 0.45)",
              color: "rgb(236, 236, 241)",
              backdropFilter: "blur(8px)",
            }}
            title="Scroll to latest"
          >
            <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
              <path d="M12 5v14M19 12l-7 7-7-7" />
            </svg>
            <span>{busy ? "travis is working…" : "jump to latest"}</span>
          </button>
        )}
      </div>

      <div
        className="pt-3 mt-3 border-t border-white/[0.04] flex flex-col gap-1 transition-all"
        style={{
          background: dropHovering
            ? "rgba(124, 92, 255, 0.08)"
            : justReadied
              ? "rgba(74, 214, 255, 0.04)"
              : "transparent",
          transition: "background 400ms ease-out, outline 400ms ease-out",
          borderRadius: dropHovering || justReadied ? 8 : 0,
          outline: dropHovering
            ? "1px dashed rgba(124, 92, 255, 0.45)"
            : justReadied
              ? "1px solid rgba(74, 214, 255, 0.30)"
              : "none",
          outlineOffset: -1,
        }}
      >
        {!empty && !busy && messages.length > 0 && messages[messages.length - 1].role === "assistant" && (
          <div className="text-[10px] text-bone-3 font-mono tracking-wider mb-1 flex items-center gap-1.5">
            <span className="inline-block h-1 w-1 rounded-full bg-pulse-2" />
            <span>your turn</span>
          </div>
        )}
        <ActiveWorkflowPill conversationId={activeConversationId} />

        {/* v0.20.0 — proposed actions awaiting consent appear above
            the input. Confirm applies via the action handler;
            dismiss marks declined. Both refetch the list so the
            card disappears immediately on resolution. */}
        {pendingActions.length > 0 && (
          <div className="mb-3 space-y-2">
            {pendingActions.map((a) => (
              <ActionCard
                key={a.id}
                action={a}
                onConfirm={async () => {
                  try {
                    await confirmAction(a.id);
                  } catch (e) {
                    setError(e instanceof Error ? e.message : String(e));
                  }
                  refreshPendingActions();
                }}
                onDecline={async () => {
                  try {
                    await declineAction(a.id);
                  } catch (e) {
                    setError(e instanceof Error ? e.message : String(e));
                  }
                  refreshPendingActions();
                }}
              />
            ))}
          </div>
        )}

        {attachedDocs.length > 0 && (
          <>
            <div className="flex flex-wrap gap-1.5 pb-1.5">
              {attachedDocs.map((a) => {
                if (isPending(a)) {
                  return (
                    <div
                      key={a.tempId}
                      className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[11px] font-mono bg-pulse/10 border border-pulse/20 text-bone-3"
                      title={`Reading ${a.filename}…`}
                    >
                      <span className="relative inline-flex h-1.5 w-1.5">
                        <span className="absolute inset-0 rounded-full bg-pulse-2 animate-ping opacity-70" />
                        <span className="relative rounded-full bg-pulse-2 h-1.5 w-1.5" />
                      </span>
                      <span className="truncate max-w-[200px]">{a.filename}</span>
                      <span className="opacity-60">reading…</span>
                    </div>
                  );
                }
                const d = a;
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
                          prev.filter(
                            (x) => isPending(x) || (x as Document).id !== d.id,
                          ),
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
                .filter((a): a is Document => !isPending(a) && expandedDocs.has((a as Document).id))
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

        <div className="flex items-end gap-2">
          <button
            onClick={handlePickFile}
            disabled={busy}
            title="Attach file"
            className="shrink-0 h-9 w-9 flex items-center justify-center rounded-full text-bone-3 hover:text-bone-2 hover:bg-white/[0.04] disabled:opacity-50 transition-colors"
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
          <AutoGrowTextarea
            ref={inputRef}
            value={q}
            onChange={(value) => {
              pulse();
              setQ(value);
            }}
            onSubmit={submit}
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
            maxRows={8}
          />
          <button
            onClick={() => void submit()}
            disabled={busy || (!q.trim() && attachedDocs.filter((a) => !isPending(a)).length === 0)}
            title="Send (Enter)"
            className="shrink-0 h-9 px-3 flex items-center justify-center rounded-full text-[12px] font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            style={{
              background: "rgba(124, 92, 255, 0.25)",
              color: "rgb(236, 236, 241)",
              border: "1px solid rgba(124, 92, 255, 0.45)",
            }}
          >
            {busy ? (
              <span className="inline-block h-1.5 w-1.5 rounded-full bg-pulse-2 animate-pulse" />
            ) : (
              <>
                <span>send</span>
                <svg
                  viewBox="0 0 24 24"
                  width="13"
                  height="13"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  className="ml-1.5"
                  aria-hidden
                >
                  <path d="M5 12h14M13 6l6 6-6 6" />
                </svg>
              </>
            )}
          </button>
        </div>
        {error && <p className="text-warn text-xs mt-1">{error}</p>}
      </div>
    </div>
  );
}

/// Merge server thread response into existing optimistic state.
///
/// The server returns the canonical message list AFTER the user turn +
/// the assistant reply. We want to:
///   1. keep every row earlier than the optimistic intact (avoids
///      AnimatePresence churning on stable rows)
///   2. replace the optimistic row with its server-side counterpart
///      (same id-key behaviour as it had as optimistic — we just
///      stamp the optimistic id onto the server's row so React still
///      sees the SAME key, preventing the visible flash)
///   3. append every message the server has that we don't (the
///      assistant reply, any subsequent system rows)
function mergeServerThread(
  prev: ConversationMessage[],
  optimisticId: number,
  serverMessages: ConversationMessage[],
): ConversationMessage[] {
  const optimisticIdx = prev.findIndex((m) => m.id === optimisticId);
  // If we somehow lost the optimistic, just take the server canon.
  if (optimisticIdx === -1) return serverMessages;
  const preOpt = prev.slice(0, optimisticIdx);
  // Find the corresponding user turn in the server response — it's the
  // last user message with matching content. We then map subsequent
  // messages over from that index.
  const optimisticContent = prev[optimisticIdx].content;
  const matchedIdx = serverMessages
    .map((m, i) => ({ m, i }))
    .reverse()
    .find(({ m }) => m.role === "user" && m.content === optimisticContent)?.i;
  if (matchedIdx === undefined) {
    // Couldn't match — fall back to server canon to avoid divergence.
    return serverMessages;
  }
  // Stamp the server row with the optimistic id so React's keyed render
  // sees no unmount. The real db id lives in payloadJson if anyone
  // needs it, but we don't expose it — only delete() uses the id and
  // it works on optimistic rows because the next reload pulls the
  // canonical id anyway. Keep both ids: server row replaces optimistic
  // content but the optimistic id stays.
  const userRowServer = serverMessages[matchedIdx];
  const stampedUser: ConversationMessage = {
    ...userRowServer,
    id: optimisticId, // Preserve key
  };
  const tail = serverMessages.slice(matchedIdx + 1);
  return [...preOpt, stampedUser, ...tail];
}

/// v0.17.1 — merge a DB-fetched step list onto in-memory state without
/// losing live-streamed steps that arrived during the fetch.
///
/// - Steps present in both: prefer whichever has a terminal status, then
///   whichever has more notes (live events accumulate them).
/// - Steps only in DB: include.
/// - Steps only in memory: keep (the live subscription saw them before
///   the DB query observed them — they exist).
///
/// Order by startedAt ascending so the chat surface renders in the
/// expected order regardless of which side surfaced each row.
function mergeStepLists(
  prev: ParsedStep[],
  dbSteps: ParsedStep[],
  conversationId: number | null,
): ParsedStep[] {
  const byId = new Map<string, ParsedStep>();
  // Seed with DB rows so they form the baseline.
  for (const s of dbSteps) {
    if (conversationId !== null && s.conversationId !== conversationId) continue;
    byId.set(s.id, s);
  }
  // Layer in-memory rows over DB rows where applicable.
  for (const s of prev) {
    if (conversationId !== null && s.conversationId !== conversationId) continue;
    const existing = byId.get(s.id);
    if (!existing) {
      byId.set(s.id, s);
      continue;
    }
    const existingTerminal = existing.status !== "running";
    const liveTerminal = s.status !== "running";
    const pick =
      liveTerminal && !existingTerminal
        ? s
        : existingTerminal && !liveTerminal
        ? existing
        : s.notes.length > existing.notes.length
        ? s
        : existing;
    byId.set(s.id, pick);
  }
  return Array.from(byId.values()).sort((a, b) =>
    a.startedAt.localeCompare(b.startedAt),
  );
}

/// Apply a streaming step event to local state immutably.
function applyStepEvent(
  prev: ParsedStep[],
  event: StepEvent,
  conversationId: number,
): ParsedStep[] {
  if (event.event === "started") {
    if (event.conversationId !== conversationId) return prev;
    const newRow: ParsedStep = {
      id: event.stepId,
      conversationId: event.conversationId,
      parentStepId: event.parentStepId ?? null,
      kind: event.kind,
      name: event.name,
      detail: event.detail ?? null,
      status: "running",
      summary: null,
      notes: [],
      startedAt: event.startedAt,
      completedAt: null,
      durationMs: null,
    };
    if (prev.some((s) => s.id === newRow.id)) return prev;
    return [...prev, newRow];
  }
  return prev.map((s) => {
    if (s.id !== event.stepId) return s;
    if (event.event === "note") {
      return { ...s, notes: [...s.notes, event.text] };
    }
    if (event.event === "result") {
      return {
        ...s,
        status: event.status,
        summary: event.error ?? event.summary ?? null,
      };
    }
    if (event.event === "completed") {
      return {
        ...s,
        durationMs: event.durationMs,
        completedAt: new Date().toISOString(),
      };
    }
    return s;
  });
}

/// Group steps with the message they belong to (the next assistant
/// message after the step started). Returns React nodes.
///
/// IMPORTANT: timestamp comparison uses Date.parse() — SQLite's
/// CURRENT_TIMESTAMP returns "2026-06-08 12:34:56" (space delimiter)
/// while chrono RFC3339 returns "2026-06-08T12:34:56.789Z" (T delim,
/// fractional). String comparison fails (space 0x20 < T 0x54) and
/// puts every step "after" every message. Date.parse normalizes both.
function tsMs(s: string | null | undefined): number {
  if (!s) return 0;
  // SQLite "YYYY-MM-DD HH:MM:SS" → make ISO by replacing space and appending Z
  const isoish = s.includes("T") ? s : s.replace(" ", "T") + "Z";
  const parsed = Date.parse(isoish);
  return Number.isFinite(parsed) ? parsed : 0;
}

function renderTurns(
  messages: ConversationMessage[],
  steps: ParsedStep[],
  busy: boolean,
  pendingDelete: number | null,
  onDeleteRequest: (id: number) => void,
  onDeleteConfirm: () => void,
  onDeleteCancel: () => void,
  deleteCount: number,
) {
  const sortedSteps = [...steps].sort(
    (a, b) => tsMs(a.startedAt) - tsMs(b.startedAt),
  );
  // For each assistant message, take steps that started after the
  // previous message's createdAt and at-or-before this message's
  // createdAt. Tolerance: steps fired within 5s AFTER an assistant
  // message also belong to it (clock skew + write ordering).
  const SKEW_MS = 5_000;
  const nodes: React.ReactNode[] = [];
  let prevTs: number = 0;
  const consumedStepIds = new Set<string>();
  for (let i = 0; i < messages.length; i++) {
    const m = messages[i];
    const upper = tsMs(m.createdAt) + SKEW_MS;
    const isAssistant = m.role === "assistant";
    const turnSteps = isAssistant
      ? sortedSteps.filter(
          (s) =>
            !consumedStepIds.has(s.id) &&
            tsMs(s.startedAt) > prevTs &&
            tsMs(s.startedAt) <= upper,
        )
      : [];
    turnSteps.forEach((s) => consumedStepIds.add(s.id));
    const generatedIds = extractGeneratedDocumentIds(m);
    nodes.push(
      <ChatTurn
        key={m.id}
        message={m}
        steps={turnSteps}
        generatedDocumentIds={generatedIds}
        onDelete={() => onDeleteRequest(m.id)}
        pendingDelete={pendingDelete === m.id}
        deleteCount={pendingDelete === m.id ? deleteCount : 0}
        onConfirmDelete={onDeleteConfirm}
        onCancelDelete={onDeleteCancel}
      />,
    );
    prevTs = tsMs(m.createdAt);
  }
  // Live (in-progress) steps for the response we're still waiting on
  const liveSteps = sortedSteps.filter((s) => !consumedStepIds.has(s.id));
  if (busy || liveSteps.length > 0) {
    nodes.push(
      <motion.div
        key="live-turn"
        initial={{ opacity: 0, y: 4 }}
        animate={{ opacity: 1, y: 0 }}
        className="flex flex-col gap-2 items-start"
      >
        <span className="text-[9px] tracking-[0.2em] uppercase text-bone-3/70">
          travis
        </span>
        <div className="w-full max-w-[640px]">
          {liveSteps.length > 0 ? (
            <div className="space-y-0.5 mb-2">
              {/* Inline render — full ChatTurn would expect a message */}
              {liveSteps.map((s) => (
                <InlineStep key={s.id} step={s} />
              ))}
            </div>
          ) : (
            <div className="text-bone-3 text-xs flex items-center gap-2">
              <span className="inline-block h-1.5 w-1.5 rounded-full bg-pulse-2 animate-pulse" />
              thinking…
            </div>
          )}
        </div>
      </motion.div>,
    );
  }
  return nodes;
}

function InlineStep({ step }: { step: ParsedStep }) {
  const icon = (() => {
    switch (step.status) {
      case "ok":
        return <span className="text-pulse-2">✓</span>;
      case "failed":
        return <span className="text-warn">✕</span>;
      case "running":
      default:
        return (
          <span className="relative inline-flex">
            <span className="h-1.5 w-1.5 rounded-full bg-pulse-2" />
            <span className="absolute inset-0 h-1.5 w-1.5 rounded-full bg-pulse-2 animate-ping opacity-60" />
          </span>
        );
    }
  })();
  return (
    <div className="text-[11px] flex items-start gap-2 py-0.5">
      <span className="shrink-0 w-3 inline-flex items-center justify-center pt-1">
        {icon}
      </span>
      <span className="flex-1 min-w-0">
        <span className="text-bone-2">{step.name}</span>
        {step.detail && (
          <span className="text-bone-3 ml-1.5 font-mono opacity-80">
            · {step.detail}
          </span>
        )}
      </span>
    </div>
  );
}

function extractGeneratedDocumentIds(message: ConversationMessage): number[] {
  if (!message.payloadJson) return [];
  try {
    const p = JSON.parse(message.payloadJson) as Record<string, unknown>;
    const fromTop = p.generatedDocumentIds;
    const ext = p.extraction as Record<string, unknown> | undefined;
    const fromExt = ext?.generatedDocumentIds;
    const ids = (Array.isArray(fromTop) ? fromTop : fromExt) as unknown;
    if (Array.isArray(ids)) {
      return ids.filter((x): x is number => typeof x === "number");
    }
  } catch {
    /* ignore */
  }
  return [];
}

// Stub to ensure parseRow is treated as used (it's the helper for
// listSteps' return path, which we import directly).
void parseRow;
