/**
 * ApprovalMultiCard — v0.28.30 Phase C.
 *
 * Multi-step approval for compound actions. Each step requires its
 * own confirm before the whole action proceeds. When every step is
 * approved the final submit button lights up and dispatching sends
 * the final verb.
 */
import { useState } from "react";
import { useAppStore } from "../../stores/app";

interface Step {
  label: string;
  detail?: string;
  verb: string;
  approved?: boolean;
}

interface Props {
  title?: string;
  action_kind: string;
  steps: Step[];
  final_submit_verb: string;
  narration?: string;
}

export function ApprovalMultiCard({ title, action_kind, steps, final_submit_verb, narration }: Props) {
  const setPendingComposerSubmit = useAppStore((s) => s.setPendingComposerSubmit);
  const [approved, setApproved] = useState<boolean[]>(() => steps.map((s) => Boolean(s.approved)));
  const [declined, setDeclined] = useState(false);

  const allApproved = approved.every(Boolean);
  const doSubmit = () => setPendingComposerSubmit(final_submit_verb);
  const doDecline = () => {
    setDeclined(true);
    setPendingComposerSubmit(`Decline ${action_kind}`);
  };

  return (
    <div
      className="rounded-2xl overflow-hidden"
      style={{
        border: `1px solid ${declined ? "rgba(255, 155, 155, 0.35)" : "rgba(189, 158, 255, 0.32)"}`,
        background: "linear-gradient(180deg, rgba(28, 24, 40, 0.62), rgba(20, 18, 30, 0.58))",
      }}
    >
      <div className="px-4 pt-3 pb-2" style={{ borderBottom: "1px solid rgba(189, 158, 255, 0.18)" }}>
        <div className="text-[10.5px] uppercase tracking-[0.22em] font-mono mb-1" style={{ color: "rgba(189, 158, 255, 0.85)" }}>approval · {action_kind}</div>
        {title && <div className="text-[14.5px] font-medium" style={{ color: "rgb(240, 240, 246)" }}>{title}</div>}
      </div>

      <ol className="px-4 py-3 flex flex-col gap-2.5">
        {steps.map((s, i) => (
          <li key={i} className="flex items-start gap-3">
            <button
              onClick={() => setApproved((prev) => prev.map((p, j) => (j === i ? !p : p)))}
              disabled={declined}
              className="shrink-0 w-5 h-5 mt-0.5 rounded-md flex items-center justify-center disabled:opacity-40"
              style={{
                background: approved[i] ? "rgba(140, 230, 175, 0.30)" : "rgba(255, 255, 255, 0.05)",
                border: `1px solid ${approved[i] ? "rgba(140, 230, 175, 0.65)" : "rgba(255, 255, 255, 0.20)"}`,
              }}
              aria-label={approved[i] ? "Approved" : "Not yet approved"}
            >
              {approved[i] && (
                <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="rgb(140, 230, 175)" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                  <path d="M20 6L9 17l-5-5" />
                </svg>
              )}
            </button>
            <div className="flex-1 min-w-0">
              <div className="text-[13.5px] leading-snug" style={{ color: `rgba(236, 236, 241, ${approved[i] ? 0.98 : 0.85})` }}>
                {s.label}
              </div>
              {s.detail && (
                <div className="text-[12px] mt-0.5" style={{ color: "rgba(236, 236, 241, 0.6)" }}>{s.detail}</div>
              )}
            </div>
          </li>
        ))}
      </ol>

      <div className="px-4 py-3 flex justify-end gap-2" style={{ borderTop: "1px solid rgba(255, 255, 255, 0.06)" }}>
        <button
          onClick={doDecline}
          disabled={declined}
          className="px-3 py-1.5 rounded-md text-[12px] tracking-wide disabled:opacity-50"
          style={{ background: "transparent", border: "1px solid rgba(255, 155, 155, 0.4)", color: "rgba(255, 200, 200, 0.9)" }}
        >
          Decline
        </button>
        <button
          onClick={doSubmit}
          disabled={!allApproved || declined}
          className="px-4 py-1.5 rounded-md text-[12.5px] tracking-wide disabled:opacity-45 disabled:cursor-not-allowed"
          style={{
            background: allApproved ? "rgba(189, 158, 255, 0.28)" : "rgba(189, 158, 255, 0.10)",
            border: "1px solid rgba(189, 158, 255, 0.55)",
            color: "rgba(236, 236, 241, 0.94)",
          }}
        >
          {allApproved ? "Approve & proceed" : `Approve all ${approved.filter(Boolean).length}/${steps.length}`}
        </button>
      </div>

      {narration && (
        <div className="px-4 py-2 text-[12px]" style={{ color: "rgba(236, 236, 241, 0.65)", borderTop: "1px solid rgba(255, 255, 255, 0.06)" }}>{narration}</div>
      )}
    </div>
  );
}
