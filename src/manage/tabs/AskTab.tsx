import { useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { journalIngest } from "../../lib/journal";
import {
  activeConversation,
  getThread,
  type ConversationMessage,
} from "../../lib/conversation";
import { useAppStore } from "../../stores/app";

export default function AskTab() {
  const [q, setQ] = useState("");
  const [busy, setBusy] = useState(false);
  const [conversationId, setConversationId] = useState<number | null>(null);
  const [messages, setMessages] = useState<ConversationMessage[]>([]);
  const [error, setError] = useState<string | null>(null);
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
    if (!text || busy) return;
    setBusy(true);
    setError(null);
    setActivity("thinking");

    const optimistic: ConversationMessage = {
      id: -Date.now(),
      conversationId: conversationId ?? -1,
      role: "user",
      content: text,
      payloadJson: null,
      createdAt: new Date().toISOString(),
    };
    setMessages((prev) => [...prev, optimistic]);
    setQ("");

    try {
      const r = await journalIngest(text, conversationId ?? undefined);
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

      <div className="pt-3 mt-3 border-t border-white/[0.04] flex flex-col gap-1">
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
          placeholder={empty ? "Type anything…" : "Continue…"}
          disabled={busy}
          className="w-full bg-transparent px-1 py-2 text-bone text-base font-light placeholder:text-bone-3/50 focus:outline-none disabled:text-bone-2/70"
        />
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
