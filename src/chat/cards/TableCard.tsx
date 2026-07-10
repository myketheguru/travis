/**
 * TableCard — v0.28.28 Phase A.
 *
 * Tabular data with typed columns (text, number, currency, date,
 * duration, percent). Overflows scroll horizontally inside their own
 * container so the canvas never scrolls sideways.
 */
import type { TableColumn } from "../../lib/richResponse";

interface Props {
  title?: string;
  columns: TableColumn[];
  rows: (string | number | null)[][];
  narration?: string;
}

function formatCell(value: string | number | null, fmt?: TableColumn["format"]): string {
  if (value === null || value === undefined) return "—";
  if (typeof value === "string" && fmt !== "date") return value;
  switch (fmt) {
    case "currency": {
      const n = typeof value === "number" ? value : Number(value);
      if (!Number.isFinite(n)) return String(value);
      return new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" }).format(n);
    }
    case "percent": {
      const n = typeof value === "number" ? value : Number(value);
      if (!Number.isFinite(n)) return String(value);
      return `${(n * 100).toFixed(1)}%`;
    }
    case "duration": {
      const n = typeof value === "number" ? value : Number(value);
      if (!Number.isFinite(n)) return String(value);
      const min = Math.round(n / 60);
      if (min < 60) return `${min}m`;
      const h = Math.floor(min / 60);
      return `${h}h ${min - h * 60}m`;
    }
    case "date":
      return String(value).slice(0, 10);
    case "number": {
      const n = typeof value === "number" ? value : Number(value);
      if (!Number.isFinite(n)) return String(value);
      return new Intl.NumberFormat("en-US").format(n);
    }
    default:
      return String(value);
  }
}

export function TableCard({ title, columns, rows, narration }: Props) {
  return (
    <div
      className="rounded-2xl overflow-hidden"
      style={{
        border: "1px solid rgba(189, 158, 255, 0.28)",
        background: "linear-gradient(180deg, rgba(28, 24, 40, 0.55), rgba(20, 18, 30, 0.52))",
      }}
    >
      {title && (
        <div
          className="px-4 py-2.5 text-[10.5px] uppercase tracking-[0.22em] font-mono"
          style={{ color: "rgba(189, 158, 255, 0.85)", borderBottom: "1px solid rgba(189, 158, 255, 0.18)" }}
        >
          {title}
        </div>
      )}
      <div className="overflow-x-auto">
        <table className="min-w-full text-[13px]" style={{ fontVariantNumeric: "tabular-nums" }}>
          <thead>
            <tr>
              {columns.map((c) => (
                <th
                  key={c.key}
                  className="px-3 py-2 text-[11px] uppercase tracking-wider font-mono text-left whitespace-nowrap"
                  style={{
                    color: "rgba(236, 236, 241, 0.55)",
                    textAlign: c.align ?? (c.format === "number" || c.format === "currency" || c.format === "percent" ? "right" : "left"),
                    borderBottom: "1px solid rgba(255, 255, 255, 0.06)",
                    width: c.width,
                  }}
                >
                  {c.label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, ri) => (
              <tr key={ri} style={{ borderBottom: "1px solid rgba(255, 255, 255, 0.04)" }}>
                {columns.map((c, ci) => (
                  <td
                    key={c.key}
                    className="px-3 py-2 whitespace-nowrap"
                    style={{
                      color: "rgba(236, 236, 241, 0.92)",
                      textAlign: c.align ?? (c.format === "number" || c.format === "currency" || c.format === "percent" ? "right" : "left"),
                    }}
                  >
                    {formatCell(row[ci], c.format)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {narration && (
        <div className="px-4 py-2 text-[12px]" style={{ color: "rgba(236, 236, 241, 0.72)", borderTop: "1px solid rgba(255, 255, 255, 0.06)" }}>
          {narration}
        </div>
      )}
    </div>
  );
}
