/**
 * EmailPreviewCard — v0.28.29 Phase B.
 *
 * Domain card for outgoing email drafts. Header strip (to/cc/subject),
 * body (markdown-rendered when flagged), attachment chips, action pills
 * for send / edit / discard.
 */
import { useAppStore } from "../../stores/app";
import { MarkdownBody } from "../MarkdownBody";
import type { RowAction } from "../../lib/richResponse";

interface Props {
  from?: string;
  to: string;
  cc?: string;
  bcc?: string;
  subject: string;
  body: string;
  body_is_markdown?: boolean;
  attachments?: { name: string; size_bytes?: number; document_id?: number }[];
  actions?: RowAction[];
  narration?: string;
}

function formatSize(b?: number): string {
  if (!b) return "";
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${(b / 1024 / 1024).toFixed(1)} MB`;
}

export function EmailPreviewCard(props: Props) {
  const setPendingComposerSubmit = useAppStore((s) => s.setPendingComposerSubmit);
  const { from, to, cc, bcc, subject, body, body_is_markdown, attachments, actions, narration } = props;
  return (
    <div
      className="rounded-2xl overflow-hidden"
      style={{
        border: "1px solid rgba(189, 158, 255, 0.32)",
        background: "linear-gradient(180deg, rgba(28, 24, 40, 0.62), rgba(20, 18, 30, 0.58))",
      }}
    >
      <div className="px-4 pt-3 pb-2 flex items-center gap-3" style={{ borderBottom: "1px solid rgba(189, 158, 255, 0.18)" }}>
        <div className="text-[10.5px] uppercase tracking-[0.22em] font-mono" style={{ color: "rgba(189, 158, 255, 0.85)" }}>email</div>
        <div className="text-[13.5px] font-medium truncate flex-1" style={{ color: "rgb(240, 240, 246)" }}>{subject}</div>
      </div>

      <dl className="px-4 py-2 grid grid-cols-[max-content_1fr] gap-x-3 gap-y-0.5 text-[12.5px]" style={{ borderBottom: "1px solid rgba(255, 255, 255, 0.06)" }}>
        {from && (<><dt className="text-[10px] uppercase tracking-wider font-mono self-center" style={{ color: "rgba(236, 236, 241, 0.5)" }}>From</dt><dd style={{ color: "rgba(236, 236, 241, 0.85)" }}>{from}</dd></>)}
        <dt className="text-[10px] uppercase tracking-wider font-mono self-center" style={{ color: "rgba(236, 236, 241, 0.5)" }}>To</dt><dd style={{ color: "rgba(236, 236, 241, 0.92)" }}>{to}</dd>
        {cc && (<><dt className="text-[10px] uppercase tracking-wider font-mono self-center" style={{ color: "rgba(236, 236, 241, 0.5)" }}>Cc</dt><dd style={{ color: "rgba(236, 236, 241, 0.85)" }}>{cc}</dd></>)}
        {bcc && (<><dt className="text-[10px] uppercase tracking-wider font-mono self-center" style={{ color: "rgba(236, 236, 241, 0.5)" }}>Bcc</dt><dd style={{ color: "rgba(236, 236, 241, 0.85)" }}>{bcc}</dd></>)}
      </dl>

      <div className="px-4 py-3 text-[13px] leading-relaxed" style={{ color: "rgba(236, 236, 241, 0.94)" }}>
        {body_is_markdown ? <MarkdownBody text={body} /> : <div style={{ whiteSpace: "pre-wrap" }}>{body}</div>}
      </div>

      {attachments && attachments.length > 0 && (
        <div className="px-4 pb-3 flex flex-wrap gap-2" style={{ borderTop: "1px solid rgba(255, 255, 255, 0.06)", paddingTop: 10 }}>
          {attachments.map((a, i) => (
            <div
              key={i}
              className="flex items-center gap-2 px-2.5 py-1 rounded-lg text-[11.5px]"
              style={{
                background: "rgba(189, 158, 255, 0.10)",
                border: "1px solid rgba(189, 158, 255, 0.35)",
                color: "rgba(236, 236, 241, 0.85)",
              }}
            >
              <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66L9.41 17.42a2 2 0 0 1-2.83-2.83l8.49-8.49" />
              </svg>
              <span className="truncate max-w-[180px]">{a.name}</span>
              {a.size_bytes && <span style={{ color: "rgba(236, 236, 241, 0.5)" }}>{formatSize(a.size_bytes)}</span>}
            </div>
          ))}
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
