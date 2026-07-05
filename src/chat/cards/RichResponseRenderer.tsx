/**
 * Rich response renderer — routes each `MessagePart` to its card
 * component by `kind`.
 *
 * Foundation for the God's-Eye interface (see INTERFACE.md). Every
 * Travis reply flows through here. Unknown kinds fall back to a
 * text render so we never lose content, even if the LLM emits a
 * kind we haven't shipped a component for yet.
 *
 * Cards live in sibling files (MapCard.tsx, DocRefCard.tsx, etc.).
 */

import { useEffect } from "react";
import type { MessagePart, RichResponse } from "../../lib/richResponse";
import { MapCard } from "./MapCard";
import { DocRefCard } from "./DocRefCard";
import { ThreadCard } from "./ThreadCard";
import { T2tConvoCard } from "./T2tConvoCard";
import { MarkdownBody } from "../MarkdownBody";
import { useCardLifecycle } from "../../stores/cardLifecycle";

export function RichResponseRenderer({
  response,
  documentIds,
}: {
  response: RichResponse;
  documentIds?: number[];
}) {
  const resurrectMany = useCardLifecycle((s) => s.resurrectMany);

  // Shell 7 — shape-shifting resume. When Travis's response carries
  // resurrect_ids, mark those cards as resurrected so they re-appear
  // in the canvas. Fires once per response payload change.
  useEffect(() => {
    if (response.resurrect_ids && response.resurrect_ids.length > 0) {
      resurrectMany(response.resurrect_ids);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [response.resurrect_ids?.join(",")]);

  return (
    <div className="flex flex-col gap-3">
      {response.parts.map((part, i) => (
        <PartRouter key={i} part={part} documentIds={documentIds} />
      ))}
    </div>
  );
}

function PartRouter({
  part,
  documentIds,
}: {
  part: MessagePart;
  documentIds?: number[];
}) {
  switch (part.kind) {
    case "text":
      return <MarkdownBody text={part.markdown} />;

    case "map":
      return <MapCard route={part.route} narration={part.narration} />;

    case "doc_ref":
      return (
        <DocRefCard
          documentId={part.document_id}
          snippet={part.snippet}
          narration={part.narration}
        />
      );

    case "thread":
      return (
        <ThreadCard
          threadId={part.thread_id}
          title={part.title}
          summary={part.summary}
          turns={part.turns}
          pinned={part.pinned}
          narration={part.narration}
        />
      );

    case "t2t_convo":
      return (
        <T2tConvoCard
          queryId={part.query_id}
          fromDisplay={part.from_display}
          toDisplay={part.to_display}
          question={part.question}
          draftedResponse={part.drafted_response}
          finalResponse={part.final_response}
          state={part.state}
          narration={part.narration}
        />
      );

    case "entity":
    case "calendar":
    case "action_proposal":
    case "list":
    case "chart":
    case "media":
      // Not shipped yet — fall back to a small placeholder card so we
      // don't drop the content. Narration surfaces the summary.
      return <PlaceholderCard part={part} />;

    default:
      // TS should have exhausted; catch-all for LLM emitting a novel
      // kind we haven't wired.
      return <UnknownCard part={part as MessagePart} />;
  }
}

function PlaceholderCard({ part }: { part: MessagePart }) {
  const summary =
    (part as { narration?: string }).narration ||
    `${part.kind} card — coming soon`;
  return (
    <div
      className="rounded-lg border px-3 py-2 text-sm"
      style={{
        borderColor: "rgba(124, 92, 255, 0.35)",
        background: "rgba(124, 92, 255, 0.06)",
        color: "rgba(236, 236, 241, 0.85)",
      }}
    >
      <div className="text-[10px] tracking-[0.18em] uppercase font-mono opacity-60 mb-1">
        // {part.kind}
      </div>
      {summary}
    </div>
  );
}

function UnknownCard({ part }: { part: MessagePart }) {
  return (
    <div className="rounded-lg border border-white/10 bg-white/[0.02] p-3 text-xs font-mono opacity-70">
      Unknown response part: {JSON.stringify(part).slice(0, 200)}…
    </div>
  );
}
