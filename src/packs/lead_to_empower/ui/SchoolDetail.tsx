/**
 * v0.20.0 — relationship-aware drill-down for a single school.
 *
 * The user's spec: "click a school → see its engagements, hours,
 * invoices, docs, all on one page." This component takes the school
 * id (from the auto-CRUD list row click) and queries the LTE-specific
 * `lte_school_detail` Tauri command which returns the school row + all
 * direct relationships in one roundtrip.
 *
 * Layout: vertical sections. Each section shows the most recent
 * entries with a tight visual treatment so a school with months of
 * activity stays scannable.
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

interface SchoolRow {
  id: number;
  workspaceId: number;
  name: string;
  district?: string | null;
  contactName?: string | null;
  contactEmail?: string | null;
  notes?: string | null;
  createdAt: string;
  updatedAt: string;
}

interface EngagementRow {
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
  createdAt: string;
  updatedAt: string;
}

interface CoachHoursRow {
  id: number;
  coachId: number;
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

interface SchoolDetailData {
  school: SchoolRow;
  engagements: EngagementRow[];
  coachHours: CoachHoursRow[];
  invoices: InvoiceRow[];
  documents: Document[];
}

interface Props {
  id?: number;
  onClose?: () => void;
}

export default function SchoolDetail({ id, onClose }: Props) {
  const [data, setData] = useState<SchoolDetailData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (id == null) return;
    let cancelled = false;
    setLoading(true);
    invoke<SchoolDetailData>("lte_school_detail", { schoolId: id })
      .then((d) => {
        if (cancelled) return;
        setData(d);
        setError(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [id]);

  if (id == null) {
    return null;
  }

  return (
    <div className="h-full overflow-y-auto p-6">
      <button
        onClick={onClose}
        className="text-bone-3 hover:text-bone-2 text-xs flex items-center gap-1.5 transition-colors mb-4"
      >
        <span aria-hidden>←</span>
        <span>Back to schools</span>
      </button>

      {loading && (
        <div className="text-bone-3 text-xs text-center py-12">Loading…</div>
      )}
      {error && (
        <div className="text-warn text-xs text-center py-12">
          Could not load school: {error}
        </div>
      )}

      {data && (
        <div className="max-w-3xl space-y-8">
          {/* Header */}
          <header>
            <h1 className="text-bone text-2xl font-light tracking-tight">
              {data.school.name}
            </h1>
            <div className="mt-1.5 text-bone-3 text-xs flex flex-wrap gap-x-3 gap-y-1">
              {data.school.district && <span>District {data.school.district}</span>}
              {data.school.contactName && (
                <span>Contact: {data.school.contactName}</span>
              )}
              {data.school.contactEmail && <span>{data.school.contactEmail}</span>}
            </div>
            {data.school.notes && (
              <p className="mt-2 text-bone-2 text-xs italic">{data.school.notes}</p>
            )}
          </header>

          {/* Engagements — a school can have multiple ACTIVE contracts
              concurrently (same period, different products, or even
              same product across different cohorts). Split visually so
              the active set is unambiguous from the archived one. */}
          <EngagementsSection engagements={data.engagements} />

          {/* Headline counts for the school */}
          <div className="rounded-md border border-white/[0.06] bg-white/[0.015] p-4 grid grid-cols-2 sm:grid-cols-4 gap-y-3 gap-x-4">
            <Stat
              label="Active contracts"
              value={
                data.engagements.filter((e) => e.stage !== "closed").length
              }
            />
            <Stat label="Total contracts" value={data.engagements.length} />
            <Stat
              label="Sessions logged"
              value={data.coachHours.length}
            />
            <Stat label="Invoices" value={data.invoices.length} />
          </div>

          {/* Recent coach hours */}
          <Section
            title="Recent coach hours"
            count={data.coachHours.length}
            emptyText="No hours logged yet. Auto-extracts when a sign-in sheet is uploaded."
          >
            {data.coachHours.length > 0 && (
              <div className="rounded-md border border-white/[0.05] overflow-hidden">
                <table className="w-full text-[12px]">
                  <thead className="text-bone-3 text-[10px] tracking-[0.15em] uppercase">
                    <tr className="border-b border-white/[0.05]">
                      <th className="text-left px-3 py-1.5">Date</th>
                      <th className="text-left px-3 py-1.5">Coach</th>
                      <th className="text-right px-3 py-1.5">Hours</th>
                      <th className="text-left px-3 py-1.5">Note</th>
                    </tr>
                  </thead>
                  <tbody className="text-bone-2">
                    {data.coachHours.map((h) => (
                      <tr key={h.id} className="border-b border-white/[0.03] last:border-0 hover:bg-white/[0.02]">
                        <td className="px-3 py-1.5 tabular-nums">{h.sessionDate}</td>
                        <td className="px-3 py-1.5">{h.coachName ?? "—"}</td>
                        <td className="px-3 py-1.5 text-right tabular-nums">{h.hours.toFixed(1)}</td>
                        <td className="px-3 py-1.5 text-bone-3 text-[11px] truncate">{h.description ?? ""}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </Section>

          {/* Invoices */}
          <Section
            title="Invoices"
            count={data.invoices.length}
            emptyText="No invoices yet. Auto-creates as draft when Travis generates one."
          >
            {data.invoices.map((inv) => (
              <InvoiceRowCard key={inv.id} invoice={inv} />
            ))}
          </Section>

          {/* Documents */}
          <Section
            title="Linked documents"
            count={data.documents.length}
            emptyText="No docs linked to this school yet. POs, WOs, sign-in sheets, and generated invoices flow in automatically when classified."
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

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <div>
      <div className="text-bone-3 text-[10px] tracking-[0.18em] uppercase mb-1">
        {label}
      </div>
      <div className="text-bone text-[18px] tabular-nums">{value}</div>
    </div>
  );
}

/**
 * v0.20.0 — engagements grouped by active-vs-closed so the user can
 * see at a glance which contracts the school is currently delivering
 * against. A school may have multiple active contracts concurrently
 * (different products in the same period, or same product across
 * different cohorts).
 */
function EngagementsSection({ engagements }: { engagements: EngagementRow[] }) {
  const active = engagements.filter((e) => e.stage !== "closed");
  const closed = engagements.filter((e) => e.stage === "closed");
  if (engagements.length === 0) {
    return (
      <Section
        title="Engagements / Contracts"
        count={0}
        emptyText="No engagements yet. Auto-creates when Travis sees a contract or scope doc for this school."
      >
        {null}
      </Section>
    );
  }
  return (
    <section>
      <h2 className="text-bone-2 text-[11px] tracking-[0.2em] uppercase mb-2 flex items-center gap-2">
        Engagements / Contracts
        <span className="text-bone-3/60 tabular-nums normal-case tracking-normal">
          {engagements.length}
        </span>
      </h2>
      {active.length > 0 && (
        <div className="mb-3">
          <div className="text-pulse-2 text-[10px] tracking-[0.18em] uppercase mb-1.5 flex items-center gap-1.5">
            <span className="inline-block h-1.5 w-1.5 rounded-full bg-pulse" />
            Active · {active.length}
          </div>
          <div className="space-y-1.5">
            {active.map((e) => (
              <EngagementRowCard key={e.id} engagement={e} />
            ))}
          </div>
        </div>
      )}
      {closed.length > 0 && (
        <div>
          <div className="text-bone-3 text-[10px] tracking-[0.18em] uppercase mb-1.5">
            Closed · {closed.length}
          </div>
          <div className="space-y-1.5 opacity-70">
            {closed.map((e) => (
              <EngagementRowCard key={e.id} engagement={e} />
            ))}
          </div>
        </div>
      )}
    </section>
  );
}

function EngagementRowCard({ engagement }: { engagement: EngagementRow }) {
  return (
    <motion.div
      layout
      className="rounded-md border border-white/[0.06] bg-white/[0.015] p-3 flex flex-col gap-1"
    >
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-bone text-[13px] truncate">{engagement.name}</span>
        <StagePill stage={engagement.stage} />
      </div>
      <div className="text-bone-3 text-[10px] font-mono flex flex-wrap gap-x-3 gap-y-0.5">
        {engagement.contractRef && <span>Contract {engagement.contractRef}</span>}
        {engagement.schoolYear && <span>{engagement.schoolYear}</span>}
        {engagement.periodStart && engagement.periodEnd && (
          <span>
            {engagement.periodStart} → {engagement.periodEnd}
          </span>
        )}
        {engagement.ceilingCents != null && (
          <span>Ceiling: ${(engagement.ceilingCents / 100).toFixed(2)}</span>
        )}
        {engagement.metricsAgreementSigned === 1 && <span>metrics signed ✓</span>}
      </div>
      {engagement.summary && (
        <p className="text-bone-3 text-[11px] mt-0.5 whitespace-pre-line">
          {engagement.summary}
        </p>
      )}
    </motion.div>
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
      className={
        "px-2 py-0.5 rounded-full text-[10px] tracking-wider uppercase " + color
      }
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
        <div className={"text-[10px] tracking-wider uppercase mt-0.5 " + statusColor}>
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
        title="Reveal in folder"
      >
        reveal
      </button>
    </motion.div>
  );
}
