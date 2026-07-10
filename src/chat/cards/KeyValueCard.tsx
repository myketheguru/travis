/**
 * KeyValueCard — v0.28.28 Phase A.
 *
 * Compact strip of labeled facts. Use for a single entity's core
 * attributes (contact details, invoice metadata, config values).
 * Better than a Table when there's no reason to have columns.
 */
interface Props {
  title?: string;
  items: { label: string; value: string; hint?: string }[];
  narration?: string;
}

export function KeyValueCard({ title, items, narration }: Props) {
  return (
    <div
      className="rounded-2xl px-4 py-3"
      style={{
        border: "1px solid rgba(189, 158, 255, 0.28)",
        background: "linear-gradient(180deg, rgba(28, 24, 40, 0.55), rgba(20, 18, 30, 0.52))",
      }}
    >
      {title && (
        <div
          className="text-[10.5px] uppercase tracking-[0.22em] font-mono mb-2.5"
          style={{ color: "rgba(189, 158, 255, 0.85)" }}
        >
          {title}
        </div>
      )}
      <dl className="grid grid-cols-[max-content_1fr] gap-x-5 gap-y-1.5">
        {items.map((it, i) => (
          <div key={i} className="contents">
            <dt
              className="text-[11px] uppercase tracking-wider font-mono self-center"
              style={{ color: "rgba(236, 236, 241, 0.5)" }}
            >
              {it.label}
            </dt>
            <dd
              className="text-[14px] leading-snug break-words"
              style={{ color: "rgba(236, 236, 241, 0.94)" }}
            >
              {it.value}
              {it.hint && (
                <span
                  className="text-[11.5px] ml-2 font-mono"
                  style={{ color: "rgba(236, 236, 241, 0.5)" }}
                >
                  {it.hint}
                </span>
              )}
            </dd>
          </div>
        ))}
      </dl>
      {narration && (
        <div className="mt-2.5 text-[12px]" style={{ color: "rgba(236, 236, 241, 0.72)" }}>
          {narration}
        </div>
      )}
    </div>
  );
}
