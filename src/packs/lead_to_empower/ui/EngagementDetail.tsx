/**
 * v0.20.0 — engagement (contract) drill-down.
 *
 * Per user spec: "a contract is essentially everything a PO/WO
 * describes." So the engagement view leads with the typed terms
 * (period, ceiling, contract ref, school year), shows the school
 * + sibling-contract count with that school, then breaks down who's
 * worked on it (coach contributions), invoices drawn against the
 * ceiling, and linked docs.
 */
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";
import { DocumentIcon } from "../../../chat/DocumentIcon";
import {
  formatBytes,
  previewDocument,
  revealDocumentInFolder,
  type Document,
} from "../../../lib/documents";

interface Engagement {
  id: number;
  name: string;
  schoolId?: number | null;
  stage: string;
  contractRef?: string | null;
  schoolYear?: string | null;
  metricsAgreementSigned: number;
  summary?: string | null;
  periodStart?: string | null;
  periodEnd?: string | null;
  ceilingCents?: number | null;
}

interface School {
  id: number;
  name: string;
  district?: string | null;
}

interface CoachContribution {
  coachId: number;
  coachName: string;
  hoursTotal: number;
  sessions: number;
}

interface CoachHoursRow {
  id: number;
  coachName?: string | null;
  sessionDate: string;
  hours: number;
  description?: string | null;
}

interface InvoiceRow {
  id: number;
  number: string;
  recipient: string;
  periodStart: string;
  periodEnd: string;
  amountCents: number;
  status: string;
  createdAt: string;
}

interface EngagementDetailData {
  engagement: Engagement;
  school: School | null;
  coachContributions: CoachContribution[];
  coachHours: CoachHoursRow[];
  invoices: InvoiceRow[];
  documents: Document[];
  siblingEngagementsCount: number;
}

interface Props {
  id?: number;
  onClose?: () => void;
}

export default function EngagementDetail({ id, onClose }: Props) {
  const [data, setData] = useState<EngagementDetailData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (id == null) return;
    let cancelled = false;
    setLoading(true);
    invoke<EngagementDetailData>("lte_engagement_detail", { engagementId: id })
      .then((d) => {
        if (cancelled) return;
        setData(d);
        setError(null);
      })
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [id]);

  if (id == null) return null;

  const e = data?.engagement;
  const totalInvoiced = data?.invoices.reduce(
    (sum, inv) => sum + (inv.status === "void" ? 0 : inv.amountCents),
    0,
  ) ?? 0;
  const remaining =
    (e?.ceilingCents ?? 0) > 0 ? (e?.ceilingCents ?? 0) - totalInvoiced : null;

  return (
    <div className="h-full overflow-y-auto p-6">
      <button
        onClick={onClose}
        className="text-bone-3 hover:text-bone-2 text-xs flex items-center gap-1.5 transition-colors mb-4"
      >
        <span aria-hidden>←</span>
        <span>Back to engagements</span>
      </button>

      {loading && (
        <div className="text-bone-3 text-xs text-center py-12">Loading…</div>
      )}
      {error && (
        <div className="text-warn text-xs text-center py-12">{error}</div>
      )}

      {data && e && (
        <div className="max-w-3xl space-y-8">
          {/* Header */}
          <header>
            <div className="flex items-center gap-3 mb-1">
              <h1 className="text-bone text-2xl font-light tracking-tight">
                {e.name}
              </h1>
              <StagePill stage={e.stage} />
            </div>
            {data.school && (
              <div className="text-bone-3 text-xs mt-1">
                with{" "}
                <span className="text-bone-2">{data.school.name}</span>
                {data.school.district && ` · District ${data.school.district}`}
                {data.siblingEngagementsCount > 0 && (
                  <span className="ml-2 text-bone-3/70">
                    ({data.siblingEngagementsCount + 1} contract
                    {data.siblingEngagementsCount === 0 ? "" : "s"} with this
                    school)
                  </span>
                )}
              </div>
            )}
          </header>

          {/* Terms block — what the PO/WO actually says */}
          <div className="rounded-md border border-white/[0.06] bg-white/[0.015] p-4 grid grid-cols-2 sm:grid-cols-4 gap-y-3 gap-x-4">
            <Term label="Contract ref" value={e.contractRef ?? "—"} />
            <Term label="School year" value={e.schoolYear ?? "—"} />
            <Term
              label="Period"
              value={
                e.periodStart && e.periodEnd
                  ? `${e.periodStart} → ${e.periodEnd}`
                  : "—"
              }
            />
            <Term
              label="Ceiling"
              value={
                e.ceilingCents != null
                  ? `$${(e.ceilingCents / 100).toFixed(2)}`
                  : "—"
              }
            />
            <Term
              label="Invoiced"
              value={`$${(totalInvoiced / 100).toFixed(2)}`}
            />
            {remaining != null && (
              <Term
                label="Remaining"
                value={`$${(remaining / 100).toFixed(2)}`}
                tint={remaining < 0 ? "warn" : remaining === 0 ? "good" : undefined}
              />
            )}
            <Term
              label="Metrics signed"
              value={e.metricsAgreementSigned === 1 ? "Yes" : "No"}
              tint={e.metricsAgreementSigned === 1 ? "good" : undefined}
            />
          </div>

          {e.summary && (
            <p className="text-bone-2 text-[12px] whitespace-pre-line">
              {e.summary}
            </p>
          )}

          {/* Coach contributions */}
          <Section
            title="Coach contributions"
            count={data.coachContributions.length}
            emptyText="No coach hours logged in this engagement's window yet."
          >
            <div className="rounded-md border border-white/[0.05] overflow-hidden">
              <table className="w-full text-[12px]">
                <thead className="text-bone-3 text-[10px] tracking-[0.15em] uppercase">
                  <tr className="border-b border-white/[0.05]">
                    <th className="text-left px-3 py-1.5">Coach</th>
                    <th className="text-right px-3 py-1.5">Hours</th>
                    <th className="text-right px-3 py-1.5">Sessions</th>
                  </tr>
                </thead>
                <tbody className="text-bone-2">
                  {data.coachContributions.map((c) => (
                    <tr
                      key={c.coachId}
                      className="border-b border-white/[0.03] last:border-0"
                    >
                      <td className="px-3 py-1.5">{c.coachName}</td>
                      <td className="px-3 py-1.5 text-right tabular-nums">
                        {c.hoursTotal.toFixed(1)}
                      </td>
                      <td className="px-3 py-1.5 text-right tabular-nums">
                        {c.sessions}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Section>

          {/* Invoices */}
          <Section
            title="Invoices against this contract"
            count={data.invoices.length}
            emptyText="No invoices drawn against this contract yet."
          >
            {data.invoices.map((inv) => (
              <InvoiceRowCard key={inv.id} invoice={inv} />
            ))}
          </Section>

          {/* Documents */}
          <Section
            title="Linked documents"
            count={data.documents.length}
            emptyText="No docs linked yet. PO/WO/contract docs flow in automatically when classified."
          >
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
              {data.documents.map((d) => (
                <DocRowCard key={d.id} doc={d} />
              ))}
            </div>
          </Section>
        </div>
      )}
    </div>
  );
}

function Term({
  label,
  value,
  tint,
}: {
  label: string;
  value: string;
  tint?: "good" | "warn";
}) {
  const tintCls =
    tint === "good"
      ? "text-good"
      : tint === "warn"
      ? "text-warn"
      : "text-bone";
  return (
    <div>
      <div className="text-bone-3 text-[10px] tracking-[0.18em] uppercase mb-1">
        {label}
      </div>
      <div className={`text-[13px] tabular-nums ${tintCls}`}>{value}</div>
    </div>
  );
}

function Section({
  title,
  count,
  emptyText,
  children,
}: {
  title: string;
  count: number;
  emptyText: string;
  children: React.ReactNode;
}) {
  return (
    <section>
      <h2 className="text-bone-2 text-[11px] tracking-[0.2em] uppercase mb-2 flex items-center gap-2">
        {title}
        <span className="text-bone-3/60 tabular-nums normal-case tracking-normal">
          {count}
        </span>
      </h2>
      {count === 0 ? (
        <p className="text-bone-3/70 text-[11px] italic">{emptyText}</p>
      ) : (
        <div className="space-y-1.5">{children}</div>
      )}
    </section>
  );
}

function StagePill({ stage }: { stage: string }) {
  const color =
    stage === "closed"
      ? "bg-white/[0.05] text-bone-3"
      : stage === "accountable"
      ? "bg-pulse/[0.12] text-pulse-2"
      : stage === "action_planning"
      ? "bg-warn/[0.10] text-warn"
      : "bg-white/[0.04] text-bone-2";
  return (
    <span
      className={`px-2 py-0.5 rounded-full text-[10px] tracking-wider uppercase ${color}`}
    >
      {stage.replace("_", " ")}
    </span>
  );
}

function InvoiceRowCard({ invoice }: { invoice: InvoiceRow }) {
  const status = invoice.status.toLowerCase();
  const statusColor =
    status === "paid"
      ? "text-good"
      : status === "sent"
      ? "text-pulse-2"
      : status === "void"
      ? "text-bone-3"
      : "text-warn";
  return (
    <motion.div
      layout
      className="rounded-md border border-white/[0.06] bg-white/[0.015] p-3 flex items-start justify-between gap-3"
    >
      <div className="flex-1 min-w-0">
        <div className="text-bone text-[13px] tabular-nums">{invoice.number}</div>
        <div className="text-bone-3 text-[10px] font-mono mt-0.5">
          {invoice.periodStart} → {invoice.periodEnd}
        </div>
      </div>
      <div className="text-right shrink-0">
        <div className="text-bone text-[13px] tabular-nums">
          ${(invoice.amountCents / 100).toFixed(2)}
        </div>
        <div className={`text-[10px] tracking-wider uppercase mt-0.5 ${statusColor}`}>
          {invoice.status}
        </div>
      </div>
    </motion.div>
  );
}

function DocRowCard({ doc }: { doc: Document }) {
  return (
    <motion.div
      layout
      className="rounded-md border border-white/[0.06] bg-white/[0.015] p-2.5 flex items-center gap-2"
    >
      <span className="text-bone-2 shrink-0">
        <DocumentIcon kind={doc.kind} mimeType={doc.mimeType} size={16} />
      </span>
      <button
        onClick={() => previewDocument(doc.id).catch(() => {})}
        className="flex-1 min-w-0 text-left hover:text-pulse"
      >
        <div className="text-bone text-[12px] truncate">{doc.displayName}</div>
        <div className="text-bone-3 text-[10px] font-mono">
          {doc.kind} · {formatBytes(doc.sizeBytes)}
        </div>
      </button>
      <button
        onClick={() => revealDocumentInFolder(doc.id).catch(() => {})}
        className="text-bone-3 hover:text-bone-2 text-[10px] shrink-0 px-1.5 py-0.5 rounded hover:bg-white/[0.05]"
      >
        reveal
      </button>
    </motion.div>
  );
}
