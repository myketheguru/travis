/**
 * v0.18.3 — searchable conversation switcher.
 *
 * Dropdown anchored to the chat header. Shows recent conversations
 * with their first-user-message preview snippet (since most threads
 * don't have explicit titles). Typing in the search input filters
 * via the backend's `list_conversations_for_switcher` which does a
 * case-insensitive substring match against the title AND any message
 * body content — so a user searching "IS 217" or "Wallace Ave" lands
 * on the right thread even if the title doesn't say so.
 *
 * Selecting a row sets `activeConversationId` (which AskTab observes
 * to remount the chat with the new thread). The "+ New chat" item
 * sets `activeConversationId = null` — fresh thread.
 */
import { useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  listConversationsForSwitcher,
  type ConversationListItem,
} from "../lib/conversation";
import { useAppStore } from "../stores/app";

interface Props {
  /// Optional — render a different label when the trigger is collapsed.
  className?: string;
}

export function ConversationSwitcher({ className }: Props) {
  const activeConversationId = useAppStore((s) => s.activeConversationId);
  const setActiveConversationId = useAppStore((s) => s.setActiveConversationId);

  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<ConversationListItem[]>([]);
  const [loading, setLoading] = useState(false);
  const wrapperRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Close on click outside
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      const el = wrapperRef.current;
      if (el && !el.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  // Focus the search input when the dropdown opens
  useEffect(() => {
    if (open) {
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  // Fetch on open + on query change (debounced lightly)
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    const handle = setTimeout(async () => {
      setLoading(true);
      try {
        const rows = await listConversationsForSwitcher(query, 50);
        if (!cancelled) setItems(rows);
      } catch {
        if (!cancelled) setItems([]);
      } finally {
        if (!cancelled) setLoading(false);
      }
    }, 150);
    return () => {
      cancelled = true;
      clearTimeout(handle);
    };
  }, [open, query]);

  const activeLabel = (() => {
    if (!activeConversationId) return "New chat";
    const row = items.find((r) => r.id === activeConversationId);
    return labelFor(row) ?? `Conversation #${activeConversationId}`;
  })();

  const handlePick = (id: number | null) => {
    setActiveConversationId(id);
    setOpen(false);
    setQuery("");
  };

  return (
    <div ref={wrapperRef} className={"relative " + (className ?? "")}>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-2 px-2.5 py-1 rounded-md text-[12px] text-bone-2 hover:bg-white/[0.04] transition-colors max-w-[320px]"
        title="Switch conversation"
      >
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
          <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
        </svg>
        <span className="truncate font-medium">{activeLabel}</span>
        <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden className="opacity-60">
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.12 }}
            className="absolute z-40 mt-1 left-0 w-[360px] rounded-lg shadow-xl bg-[#0c0d11] border border-white/[0.08]"
          >
            <div className="p-2 border-b border-white/[0.06]">
              <input
                ref={inputRef}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search conversations…"
                className="w-full bg-white/[0.03] rounded-md px-2.5 py-1.5 text-[12px] text-bone outline-none focus:bg-white/[0.05] placeholder:text-bone-3/60"
              />
            </div>
            <button
              type="button"
              onClick={() => handlePick(null)}
              className="w-full px-3 py-2 text-left text-[12px] hover:bg-white/[0.04] border-b border-white/[0.04] flex items-center gap-2 text-bone-2"
            >
              <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                <line x1="12" y1="5" x2="12" y2="19" />
                <line x1="5" y1="12" x2="19" y2="12" />
              </svg>
              <span className="font-medium">New chat</span>
            </button>
            <div className="max-h-[360px] overflow-y-auto py-1">
              {loading && items.length === 0 && (
                <div className="px-3 py-3 text-[11px] text-bone-3/60 text-center">
                  Loading…
                </div>
              )}
              {!loading && items.length === 0 && (
                <div className="px-3 py-3 text-[11px] text-bone-3/60 text-center">
                  {query ? "No matches." : "No conversations yet."}
                </div>
              )}
              {items.map((row) => {
                const isActive = row.id === activeConversationId;
                return (
                  <button
                    key={row.id}
                    type="button"
                    onClick={() => handlePick(row.id)}
                    className={
                      "w-full px-3 py-2 text-left flex flex-col gap-0.5 hover:bg-white/[0.04] " +
                      (isActive ? "bg-white/[0.05]" : "")
                    }
                  >
                    <div className="flex items-baseline justify-between gap-2">
                      <span className="text-[12.5px] text-bone truncate flex-1">
                        {labelFor(row) ?? "Untitled"}
                      </span>
                      <span className="text-[10px] text-bone-3/60 tabular-nums shrink-0">
                        {formatRelativeAge(row.updatedAt)}
                      </span>
                    </div>
                    {row.preview && (
                      <span className="text-[11px] text-bone-3/70 truncate">
                        {row.preview}
                      </span>
                    )}
                    <span className="text-[10px] text-bone-3/40 tabular-nums">
                      {row.messageCount}{" "}
                      {row.messageCount === 1 ? "message" : "messages"}
                    </span>
                  </button>
                );
              })}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function labelFor(row: ConversationListItem | undefined): string | null {
  if (!row) return null;
  if (row.title && row.title.trim()) return row.title.trim();
  if (row.preview && row.preview.trim()) return row.preview.trim();
  return null;
}

function formatRelativeAge(iso: string): string {
  const now = Date.now();
  const tsRaw = iso.includes("T") ? iso : iso.replace(" ", "T") + "Z";
  const t = Date.parse(tsRaw);
  if (!Number.isFinite(t)) return "";
  const diffMin = Math.max(0, Math.floor((now - t) / 60000));
  if (diffMin < 1) return "just now";
  if (diffMin < 60) return `${diffMin}m`;
  const diffH = Math.floor(diffMin / 60);
  if (diffH < 24) return `${diffH}h`;
  const diffD = Math.floor(diffH / 24);
  if (diffD < 7) return `${diffD}d`;
  const diffW = Math.floor(diffD / 7);
  if (diffW < 5) return `${diffW}w`;
  const diffMo = Math.floor(diffD / 30);
  return `${diffMo}mo`;
}
