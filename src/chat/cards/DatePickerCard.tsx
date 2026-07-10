/**
 * DatePickerCard — v0.28.30 Phase C.
 *
 * Inline date picker using native input[type=date] for OS-consistent
 * calendar UI. Submits ISO date + user-friendly verb on select.
 */
import { useState } from "react";
import { useAppStore } from "../../stores/app";

interface Props {
  prompt?: string;
  value?: string;
  min?: string;
  max?: string;
  submit_verb?: string;
  narration?: string;
}

function friendly(iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  return d.toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric", year: "numeric" });
}

export function DatePickerCard({ prompt, value, min, max, submit_verb, narration }: Props) {
  const setPendingComposerSubmit = useAppStore((s) => s.setPendingComposerSubmit);
  const [v, setV] = useState<string>(value ?? new Date().toISOString().slice(0, 10));
  const doSubmit = () => {
    const verb = submit_verb ?? "set date to";
    setPendingComposerSubmit(`${verb} ${v}`);
  };
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
      <div className="flex items-center gap-2">
        <input
          type="date"
          value={v}
          min={min}
          max={max}
          onChange={(e) => setV(e.target.value)}
          className="flex-1 rounded-md px-3 py-2 text-[13px] outline-none"
          style={{
            background: "rgba(0, 0, 0, 0.25)",
            border: "1px solid rgba(189, 158, 255, 0.35)",
            color: "rgba(236, 236, 241, 0.95)",
            colorScheme: "dark",
          }}
        />
        <button
          onClick={doSubmit}
          className="px-3 py-2 rounded-md text-[12px] tracking-wide shrink-0"
          style={{ background: "rgba(189, 158, 255, 0.22)", border: "1px solid rgba(189, 158, 255, 0.55)", color: "rgba(236, 236, 241, 0.94)" }}
        >
          Set
        </button>
      </div>
      <div className="text-[12px] mt-2 font-mono" style={{ color: "rgba(236, 236, 241, 0.7)" }}>
        {friendly(v)}
      </div>
      {narration && (
        <div className="mt-2 text-[12px]" style={{ color: "rgba(236, 236, 241, 0.65)" }}>{narration}</div>
      )}
    </div>
  );
}
