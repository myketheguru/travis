/**
 * CalloutCard — v0.28.28 Phase A.
 *
 * Semantic message box. Use when Travis needs to flag something
 * (info/warn/success/error) distinctly from prose. Not for errors
 * the system throws — those are for the health banner — but for
 * things Travis wants to call out: "this is time-sensitive",
 * "your account is on the free tier", "operation completed".
 */
interface Props {
  severity: "info" | "warn" | "success" | "error";
  title?: string;
  body: string;
  narration?: string;
}

const styles: Record<
  Props["severity"],
  {
    border: string;
    background: string;
    dot: string;
    accent: string;
    label: string;
    icon: string;
  }
> = {
  info: {
    border: "rgba(110, 196, 232, 0.42)",
    background: "rgba(110, 196, 232, 0.09)",
    dot: "rgb(110, 196, 232)",
    accent: "rgb(146, 210, 240)",
    label: "info",
    icon: "M12 8v4M12 16h.01",
  },
  warn: {
    border: "rgba(255, 210, 130, 0.42)",
    background: "rgba(255, 210, 130, 0.09)",
    dot: "rgb(255, 210, 130)",
    accent: "rgb(255, 220, 155)",
    label: "heads up",
    icon: "M12 9v4M12 17h.01M10.29 3.86l-8.32 14.44A1 1 0 0 0 2.85 20h18.3a1 1 0 0 0 .88-1.7L13.71 3.86a1 1 0 0 0-1.72 0z",
  },
  success: {
    border: "rgba(140, 230, 175, 0.42)",
    background: "rgba(140, 230, 175, 0.09)",
    dot: "rgb(140, 230, 175)",
    accent: "rgb(160, 240, 190)",
    label: "done",
    icon: "M20 6L9 17l-5-5",
  },
  error: {
    border: "rgba(255, 155, 155, 0.42)",
    background: "rgba(255, 155, 155, 0.09)",
    dot: "rgb(255, 155, 155)",
    accent: "rgb(255, 180, 180)",
    label: "issue",
    icon: "M12 8v4M12 16h.01M12 22c5.5 0 10-4.5 10-10S17.5 2 12 2 2 6.5 2 12s4.5 10 10 10z",
  },
};

export function CalloutCard({ severity, title, body, narration }: Props) {
  const s = styles[severity];
  return (
    <div
      className="rounded-2xl px-4 py-3 flex items-start gap-3"
      style={{ border: `1px solid ${s.border}`, background: s.background }}
    >
      <div className="shrink-0 mt-0.5 h-6 w-6 rounded-lg flex items-center justify-center" style={{ background: "rgba(0,0,0,0.25)", color: s.accent }}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
          <path d={s.icon} />
        </svg>
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-[10.5px] uppercase tracking-[0.22em] font-mono mb-0.5" style={{ color: s.accent }}>
          {title ?? s.label}
        </div>
        <div className="text-[13.5px] leading-relaxed" style={{ color: "rgba(236, 236, 241, 0.94)" }}>
          {body}
        </div>
        {narration && narration !== body && (
          <div className="mt-1.5 text-[12px]" style={{ color: "rgba(236, 236, 241, 0.65)" }}>
            {narration}
          </div>
        )}
      </div>
    </div>
  );
}
