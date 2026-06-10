/**
 * v0.20.0 — coach drill-down.
 *
 * Per user spec: "list them, see how many schools/contracts they've
 * supported, how many hours done." Lead with the totals (schools,
 * contracts, hours, sessions), then break down per school with first
 * + last session date, then the engagements they've touched, then
 * recent hours.
 */
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";

interface Coach {
  id: number;
  name: string;
  email?: string | null;
  rateCents?: number | null;
  notes?: string | null;
}

interface CoachSchoolStint {
  schoolId: number;
  schoolName: string;
  hoursTotal: number;
  sessions: number;
  firstSessionDate?: string | null;
  lastSessionDate?: string | null;
}

interface Engagement {
  id: number;
  name: string;
  schoolId?: number | null;
  stage: string;
  contractRef?: string | null;
  schoolYear?: string | null;
  periodStart?: string | null;
  periodEnd?: string | null;
}

interface CoachHoursRow {
  id: number;
  sessionDate: string;
  hours: number;
  description?: string | null;
}

interface CoachDetailData {
  coach: Coach;
  schoolsSupportedCount: number;
  engagementsCount: number;
  totalHours: number;
  sessionsCount: number;
  schools: CoachSchoolStint[];
  engagements: Engagement[];
  recentHours: CoachHoursRow[];
}

interface Props {
  id?: number;
  onClose?: () => void;
}

export default function CoachDetail({ id, onClose }: Props) {
  const [data, setData] = useState<CoachDetailData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (id == null) return;
    let cancelled = false;
    setLoading(true);
    invoke<CoachDetailData>("lte_coach_detail", { coachId: id })
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

  return (
    <div className="h-full overflow-y-auto p-6">
      <button
        onClick={onClose}
        className="text-bone-3 hover:text-bone-2 text-xs flex items-center gap-1.5 transition-colors mb-4"
      >
        <span aria-hidden>←</span>
        <span>Back to coaches</span>
      </button>

      {loading && <div className="text-bone-3 text-xs text-center py-12">Loading…</div>}
      {error && <div className="text-warn text-xs text-center py-12">{error}</div>}

      {data && (
        <div className="max-w-3xl space-y-8">
          {/* Header */}
          <header>
            <h1 className="text-bone text-2xl font-light tracking-tight">
              {data.coach.name}
            </h1>
            <div className="mt-1.5 text-bone-3 text-xs flex flex-wrap gap-x-3 gap-y-1">
              {data.coach.email && <span>{data.coach.email}</span>}
              {data.coach.rateCents != null && data.coach.rateCents > 0 && (
                <span>Rate: ${(data.coach.rateCents / 100).toFixed(2)}/hr</span>
              )}
            </div>
            {data.coach.notes && (
              <p className="mt-2 text-bone-2 text-xs italic">{data.coach.notes}</p>
            )}
          </header>

          {/* Headline numbers */}
          <div className="rounded-md border border-white/[0.06] bg-white/[0.015] p-4 grid grid-cols-2 sm:grid-cols-4 gap-y-3 gap-x-4">
            <Stat label="Schools supported" value={data.schoolsSupportedCount} />
            <Stat label="Contracts touched" value={data.engagementsCount} />
            <Stat label="Total hours" value={data.totalHours.toFixed(1)} />
            <Stat label="Sessions" value={data.sessionsCount} />
          </div>

          {/* Schools breakdown */}
          <Section
            title="Schools"
            count={data.schools.length}
            emptyText="No school hours logged yet."
          >
            <div className="rounded-md border border-white/[0.05] overflow-hidden">
              <table className="w-full text-[12px]">
                <thead className="text-bone-3 text-[10px] tracking-[0.15em] uppercase">
                  <tr className="border-b border-white/[0.05]">
                    <th className="text-left px-3 py-1.5">School</th>
                    <th className="text-right px-3 py-1.5">Hours</th>
                    <th className="text-right px-3 py-1.5">Sessions</th>
                    <th className="text-left px-3 py-1.5">First → last</th>
                  </tr>
                </thead>
                <tbody className="text-bone-2">
                  {data.schools.map((s) => (
                    <tr
                      key={s.schoolId}
                      className="border-b border-white/[0.03] last:border-0"
                    >
                      <td className="px-3 py-1.5">{s.schoolName}</td>
                      <td className="px-3 py-1.5 text-right tabular-nums">
                        {s.hoursTotal.toFixed(1)}
                      </td>
                      <td className="px-3 py-1.5 text-right tabular-nums">
                        {s.sessions}
                      </td>
                      <td className="px-3 py-1.5 text-bone-3 text-[10px] tabular-nums">
                        {s.firstSessionDate ?? "—"}
                        {s.lastSessionDate && s.lastSessionDate !== s.firstSessionDate
                          ? ` → ${s.lastSessionDate}`
                          : ""}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Section>

          {/* Engagements / contracts they've been part of */}
          <Section
            title="Contracts touched"
            count={data.engagements.length}
            emptyText="No contracts linked yet."
          >
            {data.engagements.map((eng) => (
              <motion.div
                key={eng.id}
                layout
                className="rounded-md border border-white/[0.06] bg-white/[0.015] p-3"
              >
                <div className="flex items-baseline justify-between gap-2">
                  <span className="text-bone text-[13px] truncate">{eng.name}</span>
                  <StagePill stage={eng.stage} />
                </div>
                <div className="text-bone-3 text-[10px] font-mono mt-0.5 flex flex-wrap gap-x-3 gap-y-0.5">
                  {eng.contractRef && <span>Contract {eng.contractRef}</span>}
                  {eng.schoolYear && <span>{eng.schoolYear}</span>}
                  {eng.periodStart && eng.periodEnd && (
                    <span>
                      {eng.periodStart} → {eng.periodEnd}
                    </span>
                  )}
                </div>
              </motion.div>
            ))}
          </Section>

          {/* Recent hours */}
          <Section
            title="Recent hours"
            count={data.recentHours.length}
            emptyText="No hours yet."
          >
            <div className="rounded-md border border-white/[0.05] overflow-hidden">
              <table className="w-full text-[12px]">
                <thead className="text-bone-3 text-[10px] tracking-[0.15em] uppercase">
                  <tr className="border-b border-white/[0.05]">
                    <th className="text-left px-3 py-1.5">Date</th>
                    <th className="text-right px-3 py-1.5">Hours</th>
                    <th className="text-left px-3 py-1.5">Note</th>
                  </tr>
                </thead>
                <tbody className="text-bone-2">
                  {data.recentHours.map((h) => (
                    <tr key={h.id} className="border-b border-white/[0.03] last:border-0">
                      <td className="px-3 py-1.5 tabular-nums">{h.sessionDate}</td>
                      <td className="px-3 py-1.5 text-right tabular-nums">
                        {h.hours.toFixed(1)}
                      </td>
                      <td className="px-3 py-1.5 text-bone-3 text-[11px] truncate">
                        {h.description ?? ""}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Section>
        </div>
      )}
    </div>
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
