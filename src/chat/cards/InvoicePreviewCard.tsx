/**
 * InvoicePreviewCard — v0.28.29 Phase B.
 *
 * Domain card for invoice previews. Matches the LTE invoicing shape
 * but works for any org. Status pill, from/to strip, line items,
 * subtotal/tax/total, action pills for send/download/mark-paid.
 */
import { useAppStore } from "../../stores/app";
import type { RowAction } from "../../lib/richResponse";

interface Props {
  invoice_number: string;
  status?: "draft" | "sent" | "paid" | "overdue" | "void";
  issued_at?: string;
  due_at?: string;
  from?: { name: string; address?: string; email?: string };
  to?: { name: string; address?: string; email?: string };
  line_items: {
    description: string;
    quantity?: number;
    unit?: string;
    unit_price_cents?: number;
    total_cents: number;
  }[];
  subtotal_cents?: number;
  tax_cents?: number;
  total_cents: number;
  currency?: string;
  notes?: string;
  document_id?: number;
  actions?: RowAction[];
  narration?: string;
}

const statusStyle: Record<NonNullable<Props["status"]>, { bg: string; border: string; label: string }> = {
  draft:   { bg: "rgba(236, 236, 241, 0.10)", border: "rgba(236, 236, 241, 0.30)", label: "draft" },
  sent:    { bg: "rgba(110, 196, 232, 0.14)", border: "rgba(110, 196, 232, 0.45)", label: "sent" },
  paid:    { bg: "rgba(140, 230, 175, 0.14)", border: "rgba(140, 230, 175, 0.45)", label: "paid" },
  overdue: { bg: "rgba(255, 155, 155, 0.14)", border: "rgba(255, 155, 155, 0.45)", label: "overdue" },
  void:    { bg: "rgba(236, 236, 241, 0.05)", border: "rgba(236, 236, 241, 0.15)", label: "void" },
};

function money(cents: number | undefined, currency = "USD"): string {
  if (typeof cents !== "number") return "—";
  return new Intl.NumberFormat("en-US", { style: "currency", currency }).format(cents / 100);
}

export function InvoicePreviewCard(props: Props) {
  const setPendingComposerSubmit = useAppStore((s) => s.setPendingComposerSubmit);
  const { invoice_number, status, issued_at, due_at, from, to, line_items, subtotal_cents, tax_cents, total_cents, currency, notes, actions, narration } = props;
  const st = status ? statusStyle[status] : null;
  return (
    <div
      className="rounded-2xl overflow-hidden"
      style={{
        border: "1px solid rgba(189, 158, 255, 0.32)",
        background: "linear-gradient(180deg, rgba(28, 24, 40, 0.62), rgba(20, 18, 30, 0.58))",
      }}
    >
      <div className="px-4 pt-3 pb-2 flex items-center justify-between" style={{ borderBottom: "1px solid rgba(189, 158, 255, 0.18)" }}>
        <div className="flex items-center gap-3 min-w-0">
          <div className="text-[10.5px] uppercase tracking-[0.22em] font-mono" style={{ color: "rgba(189, 158, 255, 0.85)" }}>invoice</div>
          <div className="text-[14.5px] font-medium truncate" style={{ color: "rgb(240, 240, 246)" }}>#{invoice_number}</div>
        </div>
        {st && (
          <span className="text-[10px] uppercase tracking-wider font-mono px-2 py-0.5 rounded" style={{ background: st.bg, border: `1px solid ${st.border}`, color: "rgba(236, 236, 241, 0.9)" }}>
            {st.label}
          </span>
        )}
      </div>

      {(from || to || issued_at || due_at) && (
        <div className="grid grid-cols-2 gap-4 px-4 py-3 text-[12.5px]" style={{ borderBottom: "1px solid rgba(255, 255, 255, 0.06)" }}>
          {from && (
            <div>
              <div className="text-[10px] uppercase tracking-wider font-mono mb-1" style={{ color: "rgba(236, 236, 241, 0.5)" }}>From</div>
              <div style={{ color: "rgba(236, 236, 241, 0.92)" }}>{from.name}</div>
              {from.address && <div style={{ color: "rgba(236, 236, 241, 0.6)" }}>{from.address}</div>}
            </div>
          )}
          {to && (
            <div>
              <div className="text-[10px] uppercase tracking-wider font-mono mb-1" style={{ color: "rgba(236, 236, 241, 0.5)" }}>Bill to</div>
              <div style={{ color: "rgba(236, 236, 241, 0.92)" }}>{to.name}</div>
              {to.address && <div style={{ color: "rgba(236, 236, 241, 0.6)" }}>{to.address}</div>}
            </div>
          )}
          {issued_at && (
            <div>
              <div className="text-[10px] uppercase tracking-wider font-mono mb-1" style={{ color: "rgba(236, 236, 241, 0.5)" }}>Issued</div>
              <div style={{ color: "rgba(236, 236, 241, 0.85)" }}>{issued_at.slice(0, 10)}</div>
            </div>
          )}
          {due_at && (
            <div>
              <div className="text-[10px] uppercase tracking-wider font-mono mb-1" style={{ color: "rgba(236, 236, 241, 0.5)" }}>Due</div>
              <div style={{ color: "rgba(236, 236, 241, 0.85)" }}>{due_at.slice(0, 10)}</div>
            </div>
          )}
        </div>
      )}

      <div className="px-4 py-2" style={{ fontVariantNumeric: "tabular-nums" }}>
        <div className="grid grid-cols-[1fr_max-content_max-content] gap-x-3 text-[11px] uppercase tracking-wider font-mono py-1.5" style={{ color: "rgba(236, 236, 241, 0.5)", borderBottom: "1px solid rgba(255, 255, 255, 0.06)" }}>
          <div>Item</div>
          <div className="text-right">Qty</div>
          <div className="text-right">Total</div>
        </div>
        {line_items.map((li, i) => (
          <div key={i} className="grid grid-cols-[1fr_max-content_max-content] gap-x-3 py-1.5 text-[13px]" style={{ color: "rgba(236, 236, 241, 0.92)", borderBottom: "1px solid rgba(255, 255, 255, 0.04)" }}>
            <div>{li.description}</div>
            <div className="text-right" style={{ color: "rgba(236, 236, 241, 0.7)" }}>{li.quantity ?? 1}{li.unit ? ` ${li.unit}` : ""}</div>
            <div className="text-right">{money(li.total_cents, currency)}</div>
          </div>
        ))}
      </div>

      <div className="px-4 py-3 grid grid-cols-[1fr_max-content] gap-y-1 text-[13px]" style={{ borderTop: "1px solid rgba(255, 255, 255, 0.06)", fontVariantNumeric: "tabular-nums" }}>
        {typeof subtotal_cents === "number" && (<><div style={{ color: "rgba(236, 236, 241, 0.65)" }}>Subtotal</div><div className="text-right" style={{ color: "rgba(236, 236, 241, 0.85)" }}>{money(subtotal_cents, currency)}</div></>)}
        {typeof tax_cents === "number" && (<><div style={{ color: "rgba(236, 236, 241, 0.65)" }}>Tax</div><div className="text-right" style={{ color: "rgba(236, 236, 241, 0.85)" }}>{money(tax_cents, currency)}</div></>)}
        <div className="text-[14px] font-medium pt-1" style={{ color: "rgb(240, 240, 246)", borderTop: "1px solid rgba(255, 255, 255, 0.08)" }}>Total</div>
        <div className="text-[14px] font-medium text-right pt-1" style={{ color: "rgb(240, 240, 246)", borderTop: "1px solid rgba(255, 255, 255, 0.08)" }}>{money(total_cents, currency)}</div>
      </div>

      {notes && (
        <div className="px-4 py-2.5 text-[12.5px]" style={{ color: "rgba(236, 236, 241, 0.72)", borderTop: "1px solid rgba(255, 255, 255, 0.06)" }}>
          {notes}
        </div>
      )}

      {actions && actions.length > 0 && (
        <div className="px-4 py-3 flex flex-wrap gap-2" style={{ borderTop: "1px solid rgba(255, 255, 255, 0.06)" }}>
          {actions.map((a, i) => (
            <button
              key={i}
              onClick={() => setPendingComposerSubmit(a.verb)}
              className="px-3 py-1.5 rounded-md text-[12px] tracking-wide"
              style={{
                background: a.kind === "primary" ? "rgba(189, 158, 255, 0.22)" : "rgba(255, 255, 255, 0.05)",
                border: `1px solid ${a.kind === "primary" ? "rgba(189, 158, 255, 0.55)" : "rgba(255, 255, 255, 0.14)"}`,
                color: "rgba(236, 236, 241, 0.92)",
              }}
            >
              {a.label}
            </button>
          ))}
        </div>
      )}

      {narration && (
        <div className="px-4 py-2 text-[12px]" style={{ color: "rgba(236, 236, 241, 0.65)", borderTop: "1px solid rgba(255, 255, 255, 0.06)" }}>
          {narration}
        </div>
      )}
    </div>
  );
}
