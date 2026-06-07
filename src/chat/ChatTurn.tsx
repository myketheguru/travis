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
}

/// Top-level chat turn renderer. Replaces the old MessageBubble for
/// Claude-class rendering: thinking section, named substeps, markdown
/// body, generated file cards, sources collapsible.
export function ChatTurn({ message, steps, generatedDocumentIds }: Props) {
  const isUser = message.role === "user";
  const isAssistant = message.role === "assistant";

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

  return (
    <motion.div
      initial={{ opacity: 0, y: 4 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.22 }}
      className={"flex flex-col gap-2 " + (isUser ? "items-end" : "items-start")}
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
