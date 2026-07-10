/**
 * ContactCard — v0.28.28 Phase A.
 *
 * First-class person card. Mirrors the People pack contact schema.
 * Avatar is initials-only for now; a proper photo hook comes later.
 * Actions dispatch composer-submit for the LLM to route.
 */
import { useAppStore } from "../../stores/app";
import type { RowAction } from "../../lib/richResponse";

interface Props {
  display_name: string;
  relationship?: string;
  organization?: string;
  email?: string;
  phone?: string;
  birthday?: string;
  notes?: string;
  last_contact_at?: string;
  actions?: RowAction[];
  narration?: string;
}

function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

function formatBirthday(b: string): string {
  const s = b.trim().replace(/^--/, "");
  if (s.length >= 10) {
    const d = new Date(s);
    if (!isNaN(d.getTime())) return d.toLocaleDateString("en-US", { month: "long", day: "numeric" });
  }
  const m = s.match(/^(\d{2})-(\d{2})$/);
  if (m) {
    const months = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];
    return `${months[Number(m[1]) - 1]} ${Number(m[2])}`;
  }
  return b;
}

function formatLastContact(iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  const now = Date.now();
  const days = Math.floor((now - d.getTime()) / 86400000);
  if (days === 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 30) return `${days} days ago`;
  if (days < 365) return `${Math.floor(days / 30)} months ago`;
  return `${Math.floor(days / 365)}y ago`;
}

export function ContactCard(props: Props) {
  const setPendingComposerSubmit = useAppStore((s) => s.setPendingComposerSubmit);
  const {
    display_name,
    relationship,
    organization,
    email,
    phone,
    birthday,
    notes,
    last_contact_at,
    actions,
    narration,
  } = props;
  return (
    <div
      className="rounded-2xl px-4 py-3.5"
      style={{
        border: "1px solid rgba(189, 158, 255, 0.32)",
        background: "linear-gradient(180deg, rgba(28, 24, 40, 0.62), rgba(20, 18, 30, 0.58))",
      }}
    >
      <div className="flex items-start gap-3.5">
        <div
          className="shrink-0 w-11 h-11 rounded-xl flex items-center justify-center text-[15px] font-medium"
          style={{
            background: "linear-gradient(135deg, rgba(189, 158, 255, 0.28), rgba(124, 92, 255, 0.16))",
            border: "1px solid rgba(189, 158, 255, 0.35)",
            color: "rgb(220, 210, 255)",
          }}
        >
          {initials(display_name)}
        </div>
        <div className="flex-1 min-w-0">
          <div className="text-[15.5px] font-medium truncate" style={{ color: "rgb(240, 240, 246)" }}>
            {display_name}
          </div>
          {(relationship || organization) && (
            <div className="text-[12px] font-mono mt-0.5" style={{ color: "rgba(236, 236, 241, 0.65)" }}>
              {[relationship, organization].filter(Boolean).join(" · ")}
            </div>
          )}
        </div>
      </div>

      {(email || phone || birthday || last_contact_at) && (
        <dl
          className="mt-3 grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1"
          style={{ borderTop: "1px solid rgba(255, 255, 255, 0.06)", paddingTop: 10 }}
        >
          {email && (<><dt className="text-[11px] uppercase tracking-wider font-mono self-center" style={{ color: "rgba(236, 236, 241, 0.5)" }}>Email</dt><dd className="text-[13px]" style={{ color: "rgba(236, 236, 241, 0.9)" }}>{email}</dd></>)}
          {phone && (<><dt className="text-[11px] uppercase tracking-wider font-mono self-center" style={{ color: "rgba(236, 236, 241, 0.5)" }}>Phone</dt><dd className="text-[13px]" style={{ color: "rgba(236, 236, 241, 0.9)" }}>{phone}</dd></>)}
          {birthday && (<><dt className="text-[11px] uppercase tracking-wider font-mono self-center" style={{ color: "rgba(236, 236, 241, 0.5)" }}>Birthday</dt><dd className="text-[13px]" style={{ color: "rgba(236, 236, 241, 0.9)" }}>{formatBirthday(birthday)}</dd></>)}
          {last_contact_at && (<><dt className="text-[11px] uppercase tracking-wider font-mono self-center" style={{ color: "rgba(236, 236, 241, 0.5)" }}>Last touch</dt><dd className="text-[13px]" style={{ color: "rgba(236, 236, 241, 0.75)" }}>{formatLastContact(last_contact_at)}</dd></>)}
        </dl>
      )}

      {notes && (
        <div className="mt-3 text-[12.5px] leading-relaxed" style={{ color: "rgba(236, 236, 241, 0.85)" }}>
          {notes}
        </div>
      )}

      {actions && actions.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-2">
          {actions.map((a, i) => (
            <button
              key={i}
              onClick={() => setPendingComposerSubmit(a.verb)}
              className="px-2.5 py-1 rounded-md text-[11.5px] tracking-wide"
              style={{
                background: a.kind === "primary" ? "rgba(189, 158, 255, 0.22)" : "rgba(255, 255, 255, 0.05)",
                border: `1px solid ${a.kind === "primary" ? "rgba(189, 158, 255, 0.55)" : "rgba(255, 255, 255, 0.14)"}`,
                color: "rgba(236, 236, 241, 0.9)",
              }}
            >
              {a.label}
            </button>
          ))}
        </div>
      )}

      {narration && (
        <div className="mt-3 text-[12px]" style={{ color: "rgba(236, 236, 241, 0.65)", borderTop: "1px solid rgba(255, 255, 255, 0.06)", paddingTop: 8 }}>
          {narration}
        </div>
      )}
    </div>
  );
}
