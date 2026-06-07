import { useState } from "react";
import { motion } from "framer-motion";
import { type ConversationMessage } from "../lib/conversation";
import { type ParsedStep } from "../lib/steps";
import { StepRow } from "./StepRow";
import { MarkdownBody } from "./MarkdownBody";
import { ThinkingSection } from "./ThinkingSection";
import { FileCard } from "./FileCard";

interface Props {
  message: ConversationMessage;
  /// Steps that happened between the previous message and THIS message.
  /// For user messages, this is typically empty; for assistant
  /// messages, this is the work Travis did to produce the reply.
  steps: ParsedStep[];
  /// Document ids generated as part of producing this message (parsed
  /// from payload_json's `generatedDocumentIds` field).
  generatedDocumentIds: number[];
  /// Per-message actions wired up by the parent (AskTab). Optional so
  /// other surfaces (overlay) can keep using ChatTurn without them.
  onDelete?: () => void;
  pendingDelete?: boolean;
  deleteCount?: number;
  onConfirmDelete?: () => void;
  onCancelDelete?: () => void;
}

/// Top-level chat turn renderer. Replaces the old MessageBubble for
/// Claude-class rendering: thinking section, named substeps, markdown
/// body, generated file cards, sources collapsible.
export function ChatTurn({
  message,
  steps,
  generatedDocumentIds,
  onDelete,
  pendingDelete,
  deleteCount = 0,
  onConfirmDelete,
  onCancelDelete,
}: Props) {
  const isUser = message.role === "user";
  const isAssistant = message.role === "assistant";
  const [copied, setCopied] = useState(false);
  const [hover, setHover] = useState(false);

  // Extract structured fields from payload_json if present
  const payload = parsePayload(message.payloadJson);
  const thinking = payload?.thinking ?? null;

  // Pull attached doc IDs out of the user's message text
  // ("[Attached: name (kind, doc#N), ...]") so they render as inline
  // FileCards instead of a parenthetical marker.
  const attachedIds: number[] = isUser
    ? Array.from(message.content.matchAll(/doc#(\d+)/g)).map((m) =>
        Number(m[1]),
      )
    : [];
  // Strip the marker from displayed text — files render below
  const visibleContent = isUser
    ? message.content.replace(/\n*\[Attached:[^\]]*\]\s*$/i, "").trim()
    : message.content;

  // Build parent → children tree for steps
  const stepTree = buildStepTree(steps);

  const handleCopy = async () => {
    try {
      const text = isUser
        ? message.content.replace(/\n*\[Attached:[^\]]*\]\s*$/i, "").trim()
        : message.content;
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      /* clipboard may be unavailable */
    }
  };

  const showActions =
    (onDelete || onConfirmDelete) && (hover || pendingDelete);

  return (
    <motion.div
      initial={{ opacity: 0, y: 4 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.22 }}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      className={"flex flex-col gap-2 group " + (isUser ? "items-end" : "items-start")}
    >
      <span className="text-[9px] tracking-[0.2em] uppercase text-bone-3/70">
        {isUser ? "you" : isAssistant ? "travis" : message.role}
      </span>

      {isUser ? (
        <div className="flex flex-col gap-1.5 max-w-[85%] items-end">
          {visibleContent && (
            <div
              className="rounded-2xl px-3.5 py-2 text-[14px] text-bone leading-relaxed whitespace-pre-wrap"
              style={{
                background: "rgba(124, 92, 255, 0.15)",
                border: "1px solid rgba(124, 92, 255, 0.30)",
              }}
            >
              {visibleContent}
            </div>
          )}
          {attachedIds.length > 0 && (
            <div className="flex flex-col gap-1 items-end">
              {attachedIds.map((id) => (
                <FileCard key={id} documentId={id} />
              ))}
            </div>
          )}
          {!visibleContent && attachedIds.length === 0 && (
            <div className="rounded-2xl px-3.5 py-2 text-[14px] text-bone-3 italic">
              (empty)
            </div>
          )}
        </div>
      ) : (
        <div className="w-full max-w-[640px] flex flex-col">
          {thinking && <ThinkingSection text={thinking} />}

          {stepTree.length > 0 && (
            <div className="my-2 space-y-0.5">
              {stepTree.map(({ step, children }) => (
                <StepRow key={step.id} step={step} children={children} />
              ))}
            </div>
          )}

          {visibleContent.trim() && <MarkdownBody text={visibleContent} />}

          {generatedDocumentIds.length > 0 && (
            <div className="mt-2 space-y-1">
              {generatedDocumentIds.map((id) => (
                <FileCard key={id} documentId={id} />
              ))}
            </div>
          )}
        </div>
      )}

      {showActions && (
        <div
          className={
            "flex items-center gap-1 text-[10px] text-bone-3 font-mono transition-opacity " +
            (isUser ? "self-end" : "self-start")
          }
        >
          <button
            onClick={handleCopy}
            className="px-1.5 py-0.5 rounded hover:bg-white/[0.05] hover:text-bone-2 transition-colors flex items-center gap-1"
            title="Copy"
          >
            {copied ? (
              <>
                <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                  <path d="M20 6L9 17l-5-5" />
                </svg>
                <span>copied</span>
              </>
            ) : (
              <>
                <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                  <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                  <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                </svg>
                <span>copy</span>
              </>
            )}
          </button>
          {onDelete && !pendingDelete && (
            <button
              onClick={onDelete}
              className="px-1.5 py-0.5 rounded hover:bg-warn/15 hover:text-warn transition-colors flex items-center gap-1"
              title="Delete this message and everything after"
            >
              <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                <polyline points="3 6 5 6 21 6" />
                <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
              </svg>
              <span>delete</span>
            </button>
          )}
        </div>
      )}

      {pendingDelete && (
        <div
          className={
            "mt-1 rounded-md px-3 py-2 text-[11px] flex items-center gap-3 " +
            (isUser ? "self-end" : "self-start")
          }
          style={{
            background: "rgba(255, 90, 90, 0.08)",
            border: "1px solid rgba(255, 90, 90, 0.35)",
            color: "rgb(236, 236, 241)",
          }}
        >
          <span>
            Delete this message{deleteCount > 1 ? ` and ${deleteCount - 1} following` : ""}?
            <span className="text-bone-3 ml-1">Can't be undone.</span>
          </span>
          <div className="flex items-center gap-1">
            <button
              onClick={onConfirmDelete}
              className="px-2 py-0.5 rounded font-mono tracking-wider text-[10px] uppercase"
              style={{
                background: "rgba(255, 90, 90, 0.25)",
                color: "rgb(255, 200, 200)",
                border: "1px solid rgba(255, 90, 90, 0.5)",
              }}
            >
              delete
            </button>
            <button
              onClick={onCancelDelete}
              className="px-2 py-0.5 rounded font-mono tracking-wider text-[10px] uppercase text-bone-3 hover:text-bone-2 hover:bg-white/[0.04]"
            >
              cancel
            </button>
          </div>
        </div>
      )}
    </motion.div>
  );
}

interface ParsedPayload {
  thinking?: string;
  sources?: { kind: string; sourceId: number; text: string; createdAt: string }[];
}

function parsePayload(raw?: string | null): ParsedPayload | null {
  if (!raw) return null;
  try {
    const p = JSON.parse(raw) as Record<string, unknown>;
    const result: ParsedPayload = {};
    const ext = p.extraction as Record<string, unknown> | undefined;
    // Future: extraction.thinking may carry inline thinking text
    const thinking =
      (ext?.thinking as string | undefined) ?? (p.thinking as string | undefined);
    if (thinking) result.thinking = thinking;
    const sources = (p.sources ?? ext?.memorySources) as unknown;
    if (Array.isArray(sources)) result.sources = sources as ParsedPayload["sources"];
    return result;
  } catch {
    return null;
  }
}

interface TreeNode {
  step: ParsedStep;
  children: ParsedStep[];
}

function buildStepTree(steps: ParsedStep[]): TreeNode[] {
  const byParent = new Map<string, ParsedStep[]>();
  for (const s of steps) {
    if (s.parentStepId) {
      const arr = byParent.get(s.parentStepId) ?? [];
      arr.push(s);
      byParent.set(s.parentStepId, arr);
    }
  }
  return steps
    .filter((s) => !s.parentStepId)
    .map((step) => ({ step, children: byParent.get(step.id) ?? [] }));
}
