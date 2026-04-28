import { useCallback, useEffect, useMemo, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { openPath } from "@tauri-apps/plugin-opener";
import {
  listCoaches,
  listInvoices,
  listSchools,
  transitionInvoice,
  type Coach,
  type Invoice,
  type InvoiceStatus,
  type School,
} from "../../lib/domain";
import { exportInvoicePdfPreview } from "../../lib/pdf";

const filters: { id: InvoiceStatus | "all"; label: string }[] = [
  { id: "all", label: "All" },
  { id: "draft", label: "Draft" },
  { id: "sent", label: "Sent" },
  { id: "paid", label: "Paid" },
  { id: "void", label: "Void" },
];

const statusStyles: Record<InvoiceStatus, string> = {
  draft: "border-bone-3/40 text-bone-3 bg-bone-3/[0.06]",
  sent: "border-pulse-2/40 text-pulse-2 bg-pulse-2/[0.08]",
  paid: "border-pulse/50 text-pulse bg-pulse/[0.10]",
  void: "border-warn/40 text-warn bg-warn/[0.06]",
};

const fmtMoney = (cents: number) =>
  new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
  }).format(cents / 100);

export default function InvoicesTab() {
  const [filter, setFilter] = useState<(typeof filters)[number]["id"]>("all");
  const [invoices, setInvoices] = useState<Invoice[]>([]);
  const [coaches, setCoaches] = useState<Coach[]>([]);
  const [schools, setSchools] = useState<School[]>([]);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<Invoice | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [inv, c, s] = await Promise.all([
        listInvoices(filter === "all" ? undefined : { status: filter }),
        listCoaches(),
        listSchools(),
      ]);
      setInvoices(inv);
      setCoaches(c);
      setSchools(s);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [filter]);

  useEffect(() => {
    load();
  }, [load]);

  const coachById = useMemo(() => {
    const m = new Map<number, Coach>();
    for (const c of coaches) m.set(c.id, c);
    return m;
  }, [coaches]);

  const schoolById = useMemo(() => {
    const m = new Map<number, School>();
    for (const s of schools) m.set(s.id, s);
    return m;
  }, [schools]);

  return (
    <div className="px-10 py-6 max-w-3xl mx-auto">
      <div className="flex items-center gap-2 mb-4">
        {filters.map((f) => (
          <button
            key={f.id}
            onClick={() => setFilter(f.id)}
            className={
              "px-3 py-1.5 rounded-full text-[11px] tracking-wider transition-colors " +
              (filter === f.id
                ? "bg-pulse/20 text-bone border border-pulse/30"
                : "text-bone-3 hover:text-bone-2 border border-ink-3 hover:border-ink-3/80")
            }
          >
            {f.label}
          </button>
        ))}
      </div>

      {error && <p className="text-warn text-xs mb-3">{error}</p>}

      {loading ? (
        <p className="text-bone-3 text-xs">Loading…</p>
      ) : invoices.length === 0 ? (
        <p className="text-bone-3 text-xs">No invoices for this filter.</p>
      ) : (
        <div className="flex flex-col">
          <AnimatePresence initial={false} mode="popLayout">
            {invoices.map((inv) => (
              <motion.button
                key={inv.id}
                layout
                initial={{ opacity: 0, x: -8 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: 8 }}
                transition={{ duration: 0.2 }}
                onClick={() => setSelected(inv)}
                className="text-left flex items-start gap-3 px-3 py-3 rounded-lg hover:bg-white/[0.03] transition-colors group border-b border-white/[0.03]"
              >
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-bone-2 text-sm font-mono">
                      {inv.number}
                    </span>
                    <StatusBadge status={inv.status} />
                  </div>
                  <div className="text-bone-3 text-[11px] mt-0.5">
                    {inv.recipient}
                  </div>
                  <div className="flex items-center gap-3 mt-1.5 text-[10px] font-mono text-bone-3">
                    <span>
                      {inv.periodStart} → {inv.periodEnd}
                    </span>
                    {inv.coachId && (
                      <span className="text-pulse-2/70">
                        coach: {coachById.get(inv.coachId)?.name ?? `#${inv.coachId}`}
                      </span>
                    )}
                    {inv.schoolId && (
                      <span className="text-pulse-2/70">
                        school: {schoolById.get(inv.schoolId)?.name ?? `#${inv.schoolId}`}
                      </span>
                    )}
                  </div>
                </div>
                <div className="text-bone text-sm font-mono mt-0.5">
                  {fmtMoney(inv.amountCents)}
                </div>
              </motion.button>
            ))}
          </AnimatePresence>
        </div>
      )}

      <AnimatePresence>
        {selected && (
          <InvoiceDetail
            invoice={selected}
            coach={selected.coachId ? coachById.get(selected.coachId) ?? null : null}
            school={selected.schoolId ? schoolById.get(selected.schoolId) ?? null : null}
            onClose={() => setSelected(null)}
            onTransition={async (status) => {
              try {
                const updated = await transitionInvoice(selected.id, status);
                setSelected(updated);
                load();
              } catch (e) {
                setError(e instanceof Error ? e.message : String(e));
              }
            }}
          />
        )}
      </AnimatePresence>
    </div>
  );
}

function StatusBadge({ status }: { status: InvoiceStatus }) {
  return (
    <span
      className={
        "px-2 py-0.5 rounded-full text-[9px] tracking-[0.18em] uppercase border " +
        statusStyles[status]
      }
    >
      {status}
    </span>
  );
}

function InvoiceDetail({
  invoice,
  coach,
  school,
  onClose,
  onTransition,
}: {
  invoice: Invoice;
  coach: Coach | null;
  school: School | null;
  onClose: () => void;
  onTransition: (status: InvoiceStatus) => Promise<void>;
}) {
  const [busy, setBusy] = useState<"open" | "transition" | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const openPdf = async () => {
    setBusy("open");
    setErr(null);
    try {
      const path = await exportInvoicePdfPreview(invoice.id);
      await openPath(path);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const nextStatuses: InvoiceStatus[] = (() => {
    switch (invoice.status) {
      case "draft":
        return ["sent", "void"];
      case "sent":
        return ["paid", "void"];
      case "paid":
        return [];
      case "void":
        return [];
    }
  })();

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.2 }}
      className="fixed inset-0 z-30 flex items-center justify-center px-6 py-12"
      style={{ background: "rgba(4, 3, 9, 0.7)", backdropFilter: "blur(6px)" }}
      onClick={onClose}
    >
      <motion.div
        initial={{ opacity: 0, y: 12, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 12, scale: 0.98 }}
        transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
        onClick={(e) => e.stopPropagation()}
        className="relative w-full max-w-xl rounded-2xl bg-ink/95 border border-white/[0.06] p-7 max-h-[calc(100vh-96px)] overflow-y-auto"
        style={{
          boxShadow:
            "0 30px 80px -20px rgba(0,0,0,0.7), 0 12px 30px -10px rgba(124,92,255,0.18)",
        }}
      >
        <button
          onClick={onClose}
          className="absolute top-4 right-4 text-bone-3 hover:text-bone-2 text-xs"
        >
          close ×
        </button>

        <div className="flex items-center gap-3">
          <h2 className="text-bone text-xl font-light tracking-tight font-mono">
            {invoice.number}
          </h2>
          <StatusBadge status={invoice.status} />
        </div>
        <p className="text-bone-3 text-xs mt-1">{invoice.recipient}</p>

        <div className="mt-6 grid grid-cols-2 gap-x-6 gap-y-3 text-xs">
          <Detail label="Period">
            {invoice.periodStart} → {invoice.periodEnd}
          </Detail>
          <Detail label="Hours">{invoice.hoursTotal.toFixed(2)}</Detail>
          <Detail label="Rate">{fmtMoney(invoice.rateCents)}/hr</Detail>
          <Detail label="Amount">
            <span className="text-bone text-sm">
              {fmtMoney(invoice.amountCents)}
            </span>
          </Detail>
          {coach && <Detail label="Coach">{coach.name}</Detail>}
          {school && <Detail label="School">{school.name}</Detail>}
          {invoice.issuedAt && (
            <Detail label="Issued">{invoice.issuedAt.slice(0, 10)}</Detail>
          )}
          {invoice.paidAt && (
            <Detail label="Paid">{invoice.paidAt.slice(0, 10)}</Detail>
          )}
        </div>

        {invoice.notes && (
          <div className="mt-5">
            <div className="text-bone-3 text-[10px] tracking-[0.18em] uppercase mb-1">
              Notes
            </div>
            <p className="text-bone-2 text-xs leading-relaxed whitespace-pre-wrap">
              {invoice.notes}
            </p>
          </div>
        )}

        <div className="mt-6 flex items-center gap-3 flex-wrap">
          <button
            onClick={openPdf}
            disabled={busy !== null}
            className="px-4 py-2 rounded-full bg-bone/95 text-ink text-xs font-medium hover:bg-bone disabled:opacity-30 transition-colors"
          >
            {busy === "open" ? "Opening…" : "Open PDF"}
          </button>
          {nextStatuses.map((s) => (
            <button
              key={s}
              onClick={async () => {
                if (busy) return;
                setBusy("transition");
                setErr(null);
                try {
                  await onTransition(s);
                } catch (e) {
                  setErr(e instanceof Error ? e.message : String(e));
                } finally {
                  setBusy(null);
                }
              }}
              disabled={busy !== null}
              className={
                "px-3 py-1.5 rounded-full text-[11px] border tracking-wider transition-colors disabled:opacity-30 " +
                (s === "void"
                  ? "border-warn/40 text-warn hover:bg-warn/[0.08]"
                  : "border-pulse/40 text-pulse-2 hover:bg-pulse/[0.08]")
              }
            >
              Mark {s}
            </button>
          ))}
        </div>

        {err && <p className="text-warn text-xs mt-3">{err}</p>}
        <p className="text-bone-3 text-[10px] mt-4 font-mono">
          created {invoice.createdAt.slice(0, 10)} · updated {invoice.updatedAt.slice(0, 10)}
        </p>
      </motion.div>
    </motion.div>
  );
}

function Detail({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <div className="text-bone-3 text-[10px] tracking-[0.18em] uppercase mb-0.5">
        {label}
      </div>
      <div className="text-bone-2">{children}</div>
    </div>
  );
}
