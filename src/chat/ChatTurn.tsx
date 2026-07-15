import { useEffect, useRef, useState } from "react";
import { motion } from "framer-motion";
import { type ConversationMessage } from "../lib/conversation";
import { type ParsedStep } from "../lib/steps";
import { StepRow } from "./StepRow";
import { MarkdownBody } from "./MarkdownBody";
import { RichResponseRenderer } from "./cards/RichResponseRenderer";
import { parseRichResponse } from "../lib/richResponse";
import { ThinkingSection } from "./ThinkingSection";
import { FileCard } from "./FileCard";
import { useAppStore } from "../stores/app";
import { readVoiceState } from "../lib/voice";

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
  const spokeThisMessageRef = useRef<string | null>(null);

  // v0.22.13 — auto-speak Travis's replies when voice output is on in
  // Settings. Only speaks once per message (dedup by content) so a
  // parent re-render doesn't retrigger. Non-assistant messages skip.
  //
  // v0.22.15 — if the reply is a typed RichResponse, prefer the
  // per-part `narration` fields (which are written for voice) over
  // the raw markdown. This keeps Travis from reading out machine
  // tokens like `doc#15` or the whole JSON envelope aloud.
  useEffect(() => {
    if (!isAssistant) return;
    const trimmed = message.content.trim();
    if (!trimmed) return;
    if (spokeThisMessageRef.current === trimmed) return;
    spokeThisMessageRef.current = trimmed;

    const rich = parseRichResponse(trimmed);
    let spoken: string;
    if (rich) {
      spoken = rich.parts
        .map((p) => {
          if (p.kind === "text") return p.markdown;
          return (p as { narration?: string }).narration ?? "";
        })
        .filter((s) => s.length > 0)
        .join(". ")
        .replace(/\s+/g, " ")
        .trim();
    } else {
      // Legacy markdown response — strip machine tokens for the mic.
      spoken = trimmed
        .replace(/doc#\d+/g, "")
        .replace(/```[\s\S]*?```/g, "")
        .replace(/`[^`]*`/g, "")
        .replace(/[*_#>~]/g, "")
        .replace(/\s+/g, " ")
        .trim();
    }
    if (!spoken) return;
    // v0.28.25 — modality match. Speak only when the user's last turn
    // came in via voice (speakNextResponse) OR the Settings toggle is
    // on (accessibility override for users who want everything read
    // aloud). Typed turns stay silent by default. Consume the flag so
    // the next typed turn doesn't inherit voice mode.
    const speakNext = useAppStore.getState().speakNextResponse;
    const alwaysSpeak = readVoiceState().enabled;
    if (!speakNext && !alwaysSpeak) return;
    if (speakNext) useAppStore.getState().setSpeakNextResponse(false);
    // v0.28.57 — the post-TTS auto-arm-mic dispatch is removed. Voice
    // capture now only starts from an explicit mic click or the wake
    // shortcut / (future) audio wake word. Ambient captures used to
    // hijack the next turn with background noise; explicit-only is
    // the new contract.
    void import("../lib/voice").then((mod) => mod.speak(spoken));
  }, [isAssistant, message.content]);

  // Extract structured fields from payload_json if present
  const payload = parsePayload(message.payloadJson);
  const thinking = payload?.thinking ?? null;
  const errorDetail = payload?.errorDetail ?? null;

  // Pull attached doc IDs out of the user's message text
  // ("[Attached: name (kind, doc#N), ...]") so they render as inline
  // FileCards instead of a parenthetical marker.
  const attachedIds: number[] = isUser
    ? Array.from(message.content.matchAll(/doc#(\d+)/g)).map((m) =>
        Number(m[1]),
      )
    : [];
  // v0.20.12 — assistant messages also reference docs via `doc#N`
  // markers. Until v0.20.11 we only rendered FileCards for the
  // backend-tracked `generatedDocumentIds` payload field — but that
  // field was effectively never populated on the assistant payload,
  // so the marker rode through as plain text and no card appeared.
  // Scan assistant text for the marker, union with generatedDocumentIds,
  // and render a card for each.
  const inlineAssistantIds: number[] = isAssistant
    ? Array.from(message.content.matchAll(/doc#(\d+)/g)).map((m) =>
        Number(m[1]),
      )
    : [];
  const assistantFileCardIds = isAssistant
    ? Array.from(new Set([...generatedDocumentIds, ...inlineAssistantIds]))
    : [];
  // Strip the marker from displayed text — files render below
  const visibleContent = isUser
    ? message.content.replace(/\n*\[Attached:[^\]]*\]\s*$/i, "").trim()
    : isAssistant && assistantFileCardIds.length > 0
    ? message.content.replace(/doc#\d+/g, "").replace(/[ \t]+\n/g, "\n").trim()
    : message.content;

  // Build parent → children tree for steps
  const stepTree = buildStepTree(steps);

  // v0.17.0 — distinct render for reasoning-only turns (the worker
  // emitted thinking + planning text but didn't act). Visually
  // signal that this isn't a finished answer so the user knows the
  // work isn't done.
  const isReasoningOnly =
    isAssistant && message.responseKind === "reasoning_only";

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

  const actionsAvailable = !!(onDelete || onConfirmDelete);
  const actionsVisible = actionsAvailable && (hover || pendingDelete);

  return (
    <motion.div
      data-message-id={message.id}
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

          {visibleContent.trim() && (
            isReasoningOnly ? (
              <div
                className="my-1 rounded-md border-l-2 px-3 py-2"
                style={{
                  borderColor: "rgba(124, 92, 255, 0.55)",
                  background: "rgba(124, 92, 255, 0.06)",
                }}
              >
                <div className="text-[9px] tracking-[0.2em] uppercase text-bone-3/80 mb-1">
                  Reasoning · not yet acted on
                </div>
                <div className="text-bone-2 text-[13.5px] leading-relaxed">
                  <MarkdownBody text={visibleContent} />
                </div>
              </div>
            ) : (
              // v0.22.15 — God's-Eye rendering. If the assistant emitted a
              // typed RichResponse, route each part through the renderer
              // (map card, doc ref card, etc.). Fall back to markdown when
              // the reply is genuinely prose. Only assistant messages go
              // through this — user turns stay markdown.
              (() => {
                if (isAssistant) {
                  const rich = parseRichResponse(visibleContent);
                  if (rich) {
                    return (
                      <RichResponseRenderer
                        response={rich}
                        documentIds={assistantFileCardIds}
                      />
                    );
                  }
                }
                return <MarkdownBody text={visibleContent} />;
              })()
            )
          )}

          {errorDetail && <ErrorTraceDetail detail={errorDetail} />}

          {assistantFileCardIds.length > 0 && (
            <div className="mt-2 space-y-1">
              {assistantFileCardIds.map((id) => (
                <FileCard key={id} documentId={id} />
              ))}
            </div>
          )}
        </div>
      )}

      {actionsAvailable && (
        <div
          className={
            "flex items-center gap-1 text-[10px] text-bone-3 font-mono transition-opacity duration-150 h-5 " +
            (isUser ? "self-end" : "self-start") +
            (actionsVisible ? " opacity-100" : " opacity-0 pointer-events-none")
          }
          aria-hidden={!actionsVisible}
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
  errorDetail?: ErrorDetailShape;
}

interface ErrorDetailShape {
  errMsg?: string | null;
  rawResponseSnippet?: string;
  rawResponseLength?: number;
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
    const errorDetail = p.errorDetail as ErrorDetailShape | undefined;
    if (errorDetail && typeof errorDetail === "object") {
      result.errorDetail = errorDetail;
    }
    return result;
  } catch {
    return null;
  }
}

/// Collapsed expandable error-trace block. Renders below an error
/// reply (the synthesised "Travis hit an error" message). Click to
/// expand and see the underlying LLM err_msg + a snippet of the
/// raw response, with a Copy button so the user can paste it into
/// a bug report. Persisted in payload_json's errorDetail field by
/// journal_ingest's synthesis fallback.
function ErrorTraceDetail({ detail }: { detail: ErrorDetailShape }) {
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const errMsg = detail.errMsg?.trim() ?? "";
  const rawSnippet = detail.rawResponseSnippet?.trim() ?? "";
  const rawLen = detail.rawResponseLength ?? 0;
  const hasAny = errMsg.length > 0 || rawSnippet.length > 0;
  if (!hasAny) return null;

  const copyText = [
    errMsg ? `err_msg:\n${errMsg}` : null,
    rawSnippet
      ? `raw_response (${rawLen} chars, truncated):\n${rawSnippet}`
      : null,
  ]
    .filter(Boolean)
    .join("\n\n");

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(copyText);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      /* clipboard unavailable */
    }
  };

  return (
    <div
      className="mt-2 rounded-md text-[11px] overflow-hidden"
      style={{
        background: "rgba(255, 90, 90, 0.06)",
        border: "1px solid rgba(255, 90, 90, 0.25)",
      }}
    >
      <button
        onClick={() => setExpanded((p) => !p)}
        className="w-full flex items-center justify-between px-3 py-1.5 text-left hover:bg-warn/10 transition-colors"
      >
        <span className="font-mono tracking-wider uppercase text-warn">
          <span className="mr-1.5">{expanded ? "▾" : "▸"}</span>
          error trace
        </span>
        <span className="text-bone-3 font-mono opacity-70">
          {errMsg ? "click to expand" : "raw response only"}
        </span>
      </button>
      {expanded && (
        <div className="px-3 pb-2 pt-1 space-y-2 border-t border-warn/20">
          {errMsg && (
            <div>
              <div className="text-bone-3/70 font-mono text-[9px] tracking-wider uppercase mb-1">
                err_msg
              </div>
              <pre className="whitespace-pre-wrap font-mono text-[10px] text-bone-2 leading-snug">
                {errMsg}
              </pre>
            </div>
          )}
          {rawSnippet && (
            <div>
              <div className="text-bone-3/70 font-mono text-[9px] tracking-wider uppercase mb-1">
                raw response · {rawLen} chars
              </div>
              <pre className="whitespace-pre-wrap font-mono text-[10px] text-bone-3 leading-snug max-h-[200px] overflow-y-auto">
                {rawSnippet}
              </pre>
            </div>
          )}
          <button
            onClick={handleCopy}
            className="text-[10px] font-mono tracking-wider uppercase text-bone-3 hover:text-bone-2 underline-offset-2 hover:underline"
          >
            {copied ? "copied" : "copy for bug report"}
          </button>
        </div>
      )}
    </div>
  );
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
