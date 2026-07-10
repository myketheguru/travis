/**
 * StepperCard — v0.28.28 Phase A.
 *
 * Vertical named steps with per-step status. Use for reporting
 * multi-step workflows in progress ("gathering info → drafting →
 * previewing → sent") or completed sequences.
 */
import type { StepperStep } from "../../lib/richResponse";

interface Props {
  title?: string;
  steps: StepperStep[];
  narration?: string;
}

const statusStyle: Record<
  StepperStep["status"],
  { dot: string; ring: string; label: string; textOpacity: number }
> = {
  done:    { dot: "rgb(140, 230, 175)", ring: "rgba(140, 230, 175, 0.45)", label: "done",    textOpacity: 0.75 },
  active:  { dot: "rgb(189, 158, 255)", ring: "rgba(189, 158, 255, 0.65)", label: "active",  textOpacity: 1.0 },
  pending: { dot: "rgba(236, 236, 241, 0.35)", ring: "rgba(236, 236, 241, 0.18)", label: "pending", textOpacity: 0.55 },
  failed:  { dot: "rgb(255, 155, 155)", ring: "rgba(255, 155, 155, 0.5)", label: "failed",  textOpacity: 0.9 },
};

export function StepperCard({ title, steps, narration }: Props) {
  return (
    <div
      className="rounded-2xl px-4 py-3.5"
      style={{
        border: "1px solid rgba(189, 158, 255, 0.28)",
        background: "linear-gradient(180deg, rgba(28, 24, 40, 0.55), rgba(20, 18, 30, 0.52))",
      }}
    >
      {title && (
        <div className="text-[10.5px] uppercase tracking-[0.22em] font-mono mb-3" style={{ color: "rgba(189, 158, 255, 0.85)" }}>
          {title}
        </div>
      )}
      <ol className="flex flex-col gap-2.5">
        {steps.map((s, i) => {
          const st = statusStyle[s.status];
          return (
            <li key={i} className="flex items-start gap-3">
              <div className="shrink-0 relative w-4 h-4 mt-0.5 flex items-center justify-center">
                <span
                  className="absolute inset-0 rounded-full"
                  style={{ background: st.ring, opacity: s.status === "active" ? 1 : 0.55 }}
                />
                <span
                  className="relative w-2 h-2 rounded-full"
                  style={{ background: st.dot, boxShadow: s.status === "active" ? `0 0 10px ${st.dot}` : "none" }}
                />
              </div>
              <div className="min-w-0 flex-1">
                <div
                  className="text-[13.5px] leading-snug"
                  style={{ color: `rgba(236, 236, 241, ${st.textOpacity})`, fontWeight: s.status === "active" ? 500 : 400 }}
                >
                  {s.label}
                  <span
                    className="ml-2 text-[10px] uppercase tracking-wider font-mono"
                    style={{ color: st.dot, opacity: 0.9 }}
                  >
                    {st.label}
                  </span>
                </div>
                {s.detail && (
                  <div className="text-[12px] mt-0.5" style={{ color: "rgba(236, 236, 241, 0.55)" }}>
                    {s.detail}
                  </div>
                )}
              </div>
            </li>
          );
        })}
      </ol>
      {narration && (
        <div className="mt-3 text-[12px]" style={{ color: "rgba(236, 236, 241, 0.72)" }}>
          {narration}
        </div>
      )}
    </div>
  );
}
