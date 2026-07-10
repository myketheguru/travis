/**
 * SliderCard — v0.28.30 Phase C.
 *
 * Numeric slider that submits its value as the next user turn on
 * release. Live-formatted display (currency, percent, duration) so
 * the user sees the actual number they're picking.
 */
import { useState } from "react";
import { useAppStore } from "../../stores/app";

interface Props {
  prompt?: string;
  min: number;
  max: number;
  step?: number;
  value: number;
  unit?: string;
  format?: "number" | "currency" | "percent" | "duration";
  submit_verb?: string;
  submit_template?: string;
  narration?: string;
}

function format(v: number, fmt?: Props["format"], unit?: string): string {
  switch (fmt) {
    case "currency":
      return new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" }).format(v);
    case "percent":
      return `${(v * 100).toFixed(1)}%`;
    case "duration": {
      const min = Math.round(v / 60);
      if (min < 60) return `${min}m`;
      const h = Math.floor(min / 60);
      return `${h}h ${min - h * 60}m`;
    }
    default:
      return unit ? `${new Intl.NumberFormat("en-US").format(v)} ${unit}` : new Intl.NumberFormat("en-US").format(v);
  }
}

export function SliderCard({ prompt, min, max, step, value, unit, format: fmt, submit_verb, submit_template, narration }: Props) {
  const [v, setV] = useState<number>(value);
  const setPendingComposerSubmit = useAppStore((s) => s.setPendingComposerSubmit);
  const formatted = format(v, fmt, unit);
  const doSubmit = () => {
    const verb = submit_verb ?? "set to";
    const text = submit_template
      ? submit_template.replace("$VALUE", formatted)
      : `${verb} ${formatted}`;
    setPendingComposerSubmit(text);
  };
  const percent = ((v - min) / (max - min)) * 100;
  return (
    <div
      className="rounded-2xl px-4 py-3.5"
      style={{
        border: "1px solid rgba(189, 158, 255, 0.28)",
        background: "linear-gradient(180deg, rgba(28, 24, 40, 0.55), rgba(20, 18, 30, 0.52))",
      }}
    >
      {prompt && (
        <div className="text-[13px] mb-3 leading-snug" style={{ color: "rgba(236, 236, 241, 0.9)" }}>{prompt}</div>
      )}
      <div className="flex items-baseline justify-between mb-2">
        <span className="text-[10px] uppercase tracking-wider font-mono" style={{ color: "rgba(236, 236, 241, 0.5)" }}>{format(min, fmt, unit)}</span>
        <span className="text-[19px] font-medium" style={{ color: "rgb(220, 210, 255)", fontVariantNumeric: "tabular-nums" }}>{formatted}</span>
        <span className="text-[10px] uppercase tracking-wider font-mono" style={{ color: "rgba(236, 236, 241, 0.5)" }}>{format(max, fmt, unit)}</span>
      </div>
      <div className="relative py-2">
        <input
          type="range"
          min={min}
          max={max}
          step={step ?? 1}
          value={v}
          onChange={(e) => setV(Number(e.target.value))}
          onMouseUp={doSubmit}
          onTouchEnd={doSubmit}
          onKeyUp={(e) => { if (e.key === "Enter") doSubmit(); }}
          className="w-full appearance-none bg-transparent cursor-pointer"
          style={{ height: 4 }}
        />
        <div className="absolute inset-x-0 top-1/2 -translate-y-1/2 pointer-events-none rounded-full h-1" style={{ background: "rgba(255,255,255,0.08)" }}>
          <div className="h-full rounded-full" style={{ width: `${percent}%`, background: "linear-gradient(90deg, rgba(189, 158, 255, 0.85), rgb(220, 200, 255))" }} />
        </div>
      </div>
      <button
        onClick={doSubmit}
        className="mt-2 w-full text-[12px] py-1.5 rounded-md tracking-wide"
        style={{ background: "rgba(189, 158, 255, 0.20)", border: "1px solid rgba(189, 158, 255, 0.45)", color: "rgba(236, 236, 241, 0.94)" }}
      >
        {submit_verb ?? "Set"} {formatted}
      </button>
      {narration && (
        <div className="mt-2 text-[12px]" style={{ color: "rgba(236, 236, 241, 0.65)" }}>{narration}</div>
      )}
    </div>
  );
}
