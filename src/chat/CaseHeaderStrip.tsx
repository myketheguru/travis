import { useEffect, useState } from "react";
import {
  caseForConversation,
  closeCase as closeCaseCmd,
  listOpenCases,
  type Case,
} from "../lib/cases";

interface Props {
  conversationId: number | null;
  /// Called when the user closes the active case — parent can clear
  /// its local cache of the active case.
  onClose: () => void;
  /// Called when the user switches to another case's most recent
  /// conversation. Parent should setActiveConversationId.
  onSwitchToConversation: (conversationId: number) => void;
}

/// v0.16.0 — Slim header strip above the chat transcript that shows
/// the active case (when one exists for the current conversation),
/// with a popover to switch cases or close the active one. The
/// header is only visible when there's a case; non-case conversations
/// render as before.
export function CaseHeaderStrip({
  conversationId,
  onClose,
  onSwitchToConversation,
}: Props) {
  const [activeCase, setActiveCase] = useState<Case | null>(null);
  const [popoverOpen, setPopoverOpen] = useState(false);
  const [openCases, setOpenCases] = useState<Case[]>([]);

  // Fetch the case for the current conversation whenever the
  // conversation id changes. Re-poll every 12s to catch backend
  // auto-opens that happen mid-turn.
  useEffect(() => {
    if (!conversationId) {
      setActiveCase(null);
      return;
    }
    let cancelled = false;
    const load = async () => {
      try {
        const c = await caseForConversation(conversationId);
        if (!cancelled) setActiveCase(c);
      } catch {
        /* ignore */
      }
    };
    void load();
    const id = setInterval(load, 12_000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [conversationId]);

  // Populate the popover lazily when opened.
  useEffect(() => {
    if (!popoverOpen) return;
    let cancelled = false;
    listOpenCases(20)
      .then((cs) => {
        if (!cancelled) setOpenCases(cs);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [popoverOpen]);

  if (!activeCase) return null;

  const handleClose = async () => {
    if (!activeCase) return;
    if (!confirm(`Close case "${activeCase.name}"? It will still be reachable from the switcher.`)) {
      return;
    }
    try {
      await closeCaseCmd(activeCase.id);
      setActiveCase(null);
      setPopoverOpen(false);
      onClose();
    } catch {
      /* ignore */
    }
  };

  const handlePickCase = (c: Case) => {
    try {
      const ids = JSON.parse(c.conversationIdsJson) as number[];
      const latest = ids[ids.length - 1];
      if (latest && latest !== conversationId) {
        onSwitchToConversation(latest);
      }
    } catch {
      /* ignore */
    }
    setPopoverOpen(false);
  };

  const turnsLabel = (() => {
    try {
      const ids = JSON.parse(activeCase.conversationIdsJson) as number[];
      return `${ids.length} conversation${ids.length === 1 ? "" : "s"}`;
    } catch {
      return "1 conversation";
    }
  })();

  return (
    <div
      className="relative flex items-center justify-between gap-3 px-3 py-1.5 mb-2 text-[11px] font-mono"
      style={{
        background: "rgba(124, 92, 255, 0.08)",
        border: "1px solid rgba(124, 92, 255, 0.25)",
        borderRadius: 6,
      }}
    >
      <div className="flex items-center gap-2 min-w-0 flex-1">
        <span className="text-pulse-2 tracking-wider uppercase text-[9px]">
          case
        </span>
        <span className="text-bone-2 truncate" title={activeCase.name}>
          {activeCase.name}
        </span>
        <span className="text-bone-3 opacity-70 shrink-0">
          · {turnsLabel}
        </span>
        {activeCase.summary && (
          <span
            className="text-bone-3 opacity-70 truncate hidden md:inline"
            title={activeCase.summary}
          >
            · {activeCase.summary}
          </span>
        )}
      </div>
      <div className="flex items-center gap-2 shrink-0">
        <button
          onClick={() => setPopoverOpen((p) => !p)}
          className="text-bone-3 hover:text-bone-2 underline-offset-4 hover:underline tracking-wider uppercase text-[9px]"
        >
          switch
        </button>
        <button
          onClick={handleClose}
          className="text-bone-3 hover:text-warn underline-offset-4 hover:underline tracking-wider uppercase text-[9px]"
        >
          close case
        </button>
      </div>
      {popoverOpen && openCases.length > 0 && (
        <div
          className="absolute top-full right-0 mt-1 z-10 w-[320px] max-h-[280px] overflow-y-auto rounded-md shadow-lg"
          style={{
            background: "rgba(20, 22, 30, 0.96)",
            border: "1px solid rgba(124, 92, 255, 0.3)",
            backdropFilter: "blur(8px)",
          }}
        >
          <div className="px-3 py-1.5 text-bone-3 tracking-wider uppercase text-[9px] border-b border-white/[0.05]">
            open cases
          </div>
          {openCases.map((c) => {
            const isCurrent = c.id === activeCase.id;
            return (
              <button
                key={c.id}
                onClick={() => handlePickCase(c)}
                disabled={isCurrent}
                className={
                  "block w-full text-left px-3 py-1.5 text-[11px] " +
                  (isCurrent
                    ? "bg-pulse/15 text-bone cursor-default"
                    : "hover:bg-white/[0.04] text-bone-2")
                }
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="truncate">{c.name}</span>
                  {isCurrent && (
                    <span className="text-pulse-2 text-[9px] tracking-wider uppercase shrink-0">
                      active
                    </span>
                  )}
                </div>
                {c.summary && (
                  <div className="text-bone-3 opacity-70 truncate text-[10px] mt-0.5">
                    {c.summary}
                  </div>
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
