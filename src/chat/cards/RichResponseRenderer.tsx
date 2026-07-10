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
import { ListCard } from "./ListCard";
// v0.28.28 Phase A rich-response cards
import { TableCard } from "./TableCard";
import { KeyValueCard } from "./KeyValueCard";
import { CalloutCard } from "./CalloutCard";
import { QuickReplyCard } from "./QuickReplyCard";
import { StepperCard } from "./StepperCard";
import { CodeSnippetCard } from "./CodeSnippetCard";
import { ContactCard } from "./ContactCard";
// v0.28.29 Phase B domain cards
import { InvoicePreviewCard } from "./InvoicePreviewCard";
import { EmailPreviewCard } from "./EmailPreviewCard";
import { RouteStepsCard } from "./RouteStepsCard";
import { CalendarEventCard } from "./CalendarEventCard";
// v0.28.30 Phase C interactive inputs
import { SliderCard } from "./SliderCard";
import { DatePickerCard } from "./DatePickerCard";
import { SlotFormCard } from "./SlotFormCard";
import { ApprovalMultiCard } from "./ApprovalMultiCard";
import { MarkdownBody } from "../MarkdownBody";
import { useCardLifecycle } from "../../stores/cardLifecycle";

export function RichResponseRenderer({
  response,
  documentIds,
  messageId,
}: {
  response: RichResponse;
  documentIds?: number[];
  messageId?: string;
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
        <PartRouter
          key={i}
          part={part}
          documentIds={documentIds}
          messageId={messageId}
        />
      ))}
    </div>
  );
}

function PartRouter({
  part,
  messageId,
}: {
  part: MessagePart;
  documentIds?: number[];
  messageId?: string;
}) {
  // v0.28.14 — voice-as-tool: 'silent' channel skips rendering.
  if (part.channel === "silent") return null;

  switch (part.kind) {
    case "text":
      return <MarkdownBody text={part.markdown} />;

    case "map":
      return (
        <MapCard
          route={part.route}
          place={part.place}
          narration={part.narration}
          messageId={messageId}
        />
      );

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

    case "list":
      return (
        <ListCard
          title={part.title}
          rows={part.rows}
          narration={part.narration}
        />
      );

    // v0.28.28 Phase A
    case "table":
      return (
        <TableCard
          title={part.title}
          columns={part.columns}
          rows={part.rows}
          narration={part.narration}
        />
      );
    case "keyvalue":
      return (
        <KeyValueCard title={part.title} items={part.items} narration={part.narration} />
      );
    case "callout":
      return (
        <CalloutCard
          severity={part.severity}
          title={part.title}
          body={part.body}
          narration={part.narration}
        />
      );
    case "quickreply":
      return (
        <QuickReplyCard prompt={part.prompt} options={part.options} narration={part.narration} />
      );
    case "stepper":
      return <StepperCard title={part.title} steps={part.steps} narration={part.narration} />;
    case "code_snippet":
      return (
        <CodeSnippetCard
          code={part.code}
          language={part.language}
          filename={part.filename}
          narration={part.narration}
        />
      );
    case "contact_card":
      return (
        <ContactCard
          display_name={part.display_name}
          relationship={part.relationship}
          organization={part.organization}
          email={part.email}
          phone={part.phone}
          birthday={part.birthday}
          notes={part.notes}
          last_contact_at={part.last_contact_at}
          actions={part.actions}
          narration={part.narration}
        />
      );

    // v0.28.29 Phase B
    case "invoice_preview":
      return (
        <InvoicePreviewCard
          invoice_number={part.invoice_number}
          status={part.status}
          issued_at={part.issued_at}
          due_at={part.due_at}
          from={part.from}
          to={part.to}
          line_items={part.line_items}
          subtotal_cents={part.subtotal_cents}
          tax_cents={part.tax_cents}
          total_cents={part.total_cents}
          currency={part.currency}
          notes={part.notes}
          document_id={part.document_id}
          actions={part.actions}
          narration={part.narration}
        />
      );
    case "email_preview":
      return (
        <EmailPreviewCard
          from={part.from}
          to={part.to}
          cc={part.cc}
          bcc={part.bcc}
          subject={part.subject}
          body={part.body}
          body_is_markdown={part.body_is_markdown}
          attachments={part.attachments}
          actions={part.actions}
          narration={part.narration}
        />
      );
    case "route_steps":
      return (
        <RouteStepsCard
          from_label={part.from_label}
          to_label={part.to_label}
          total_distance_meters={part.total_distance_meters}
          total_duration_seconds={part.total_duration_seconds}
          profile={part.profile}
          steps={part.steps}
          narration={part.narration}
        />
      );
    case "calendar_event":
      return (
        <CalendarEventCard
          event_id={part.event_id}
          title={part.title}
          start={part.start}
          end={part.end}
          location={part.location}
          attendees={part.attendees}
          organizer={part.organizer}
          description={part.description}
          meeting_url={part.meeting_url}
          actions={part.actions}
          narration={part.narration}
        />
      );

    // v0.28.30 Phase C
    case "slider":
      return (
        <SliderCard
          prompt={part.prompt}
          min={part.min}
          max={part.max}
          step={part.step}
          value={part.value}
          unit={part.unit}
          format={part.format}
          submit_verb={part.submit_verb}
          submit_template={part.submit_template}
          narration={part.narration}
        />
      );
    case "datepicker":
      return (
        <DatePickerCard
          prompt={part.prompt}
          value={part.value}
          min={part.min}
          max={part.max}
          submit_verb={part.submit_verb}
          narration={part.narration}
        />
      );
    case "slotform":
      return (
        <SlotFormCard
          title={part.title}
          intro={part.intro}
          fields={part.fields}
          submit_label={part.submit_label}
          submit_verb={part.submit_verb}
          narration={part.narration}
        />
      );
    case "approval_multi":
      return (
        <ApprovalMultiCard
          title={part.title}
          action_kind={part.action_kind}
          steps={part.steps}
          final_submit_verb={part.final_submit_verb}
          narration={part.narration}
        />
      );

    case "entity":
    case "calendar":
    case "action_proposal":
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
