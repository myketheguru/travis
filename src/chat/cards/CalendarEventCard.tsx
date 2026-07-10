/**
 * CalendarEventCard — v0.28.29 Phase B.
 *
 * Single-event preview (contrast with `calendar` which is a
 * time-window). Renders time slot, location, attendees, meeting
 * URL, plus action pills for RSVP / add-to-calendar / reschedule.
 */
import { useAppStore } from "../../stores/app";
import type { RowAction } from "../../lib/richResponse";

interface Props {
  event_id?: string;
  title: string;
  start: string;
  end: string;
  location?: string;
  attendees?: string[];
  organizer?: string;
  description?: string;
  meeting_url?: string;
  actions?: RowAction[];
  narration?: string;
}

function fmtRange(start: string, end: string): string {
  const s = new Date(start);
  const e = new Date(end);
  if (isNaN(s.getTime()) || isNaN(e.getTime())) return `${start} → ${end}`;
  const sameDay = s.toDateString() === e.toDateString();
  const day = s.toLocaleDateString("en-US", { weekday: "short", month: "short", day: "numeric" });
  const st = s.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
  const et = e.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
  if (sameDay) return `${day} · ${st} – ${et}`;
  const day2 = e.toLocaleDateString("en-US", { weekday: "short", month: "short", day: "numeric" });
  return `${day} ${st} → ${day2} ${et}`;
}

function initialsOf(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (!parts.length) return "?";
  return (parts[0][0] + (parts[parts.length - 1][0] ?? "")).toUpperCase();
}

export function CalendarEventCard(props: Props) {
  const setPendingComposerSubmit = useAppStore((s) => s.setPendingComposerSubmit);
  const { title, start, end, location, attendees, organizer, description, meeting_url, actions, narration } = props;
  return (
    <div
      className="rounded-2xl overflow-hidden"
      style={{
        border: "1px solid rgba(189, 158, 255, 0.32)",
        background: "linear-gradient(180deg, rgba(28, 24, 40, 0.62), rgba(20, 18, 30, 0.58))",
      }}
    >
      <div className="px-4 pt-3 pb-2.5" style={{ borderBottom: "1px solid rgba(189, 158, 255, 0.18)" }}>
        <div className="text-[10.5px] uppercase tracking-[0.22em] font-mono mb-1" style={{ color: "rgba(189, 158, 255, 0.85)" }}>event</div>
        <div className="text-[15.5px] font-medium leading-tight" style={{ color: "rgb(240, 240, 246)" }}>{title}</div>
        <div className="text-[12.5px] font-mono mt-1" style={{ color: "rgba(236, 236, 241, 0.75)" }}>
          {fmtRange(start, end)}
        </div>
      </div>

      <div className="px-4 py-3 flex flex-col gap-2 text-[13px]">
        {location && (
          <div className="flex items-start gap-2">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="rgba(236, 236, 241, 0.55)" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" style={{ marginTop: 2 }} aria-hidden>
              <path d="M12 21s-7-6.5-7-12a7 7 0 0 1 14 0c0 5.5-7 12-7 12z" />
              <circle cx="12" cy="9" r="2.5" />
            </svg>
            <span style={{ color: "rgba(236, 236, 241, 0.9)" }}>{location}</span>
          </div>
        )}
        {meeting_url && (
          <div className="flex items-start gap-2">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="rgba(236, 236, 241, 0.55)" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" style={{ marginTop: 2 }} aria-hidden>
              <path d="M23 7l-7 5 7 5V7zM1 5h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H1z" />
            </svg>
            <span style={{ color: "rgb(189, 158, 255)", wordBreak: "break-all" }}>{meeting_url}</span>
          </div>
        )}
        {organizer && (
          <div className="flex items-start gap-2">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="rgba(236, 236, 241, 0.55)" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" style={{ marginTop: 2 }} aria-hidden>
              <circle cx="12" cy="7" r="4" />
              <path d="M4 21c0-4 4-7 8-7s8 3 8 7" />
            </svg>
            <span style={{ color: "rgba(236, 236, 241, 0.85)" }}>{organizer}</span>
          </div>
        )}
      </div>

      {attendees && attendees.length > 0 && (
        <div className="px-4 pb-3 flex flex-wrap gap-1.5" style={{ borderTop: "1px solid rgba(255, 255, 255, 0.06)", paddingTop: 10 }}>
          {attendees.slice(0, 8).map((a, i) => (
            <div
              key={i}
              title={a}
              className="w-7 h-7 rounded-full flex items-center justify-center text-[10px] font-mono"
              style={{
                background: "rgba(189, 158, 255, 0.15)",
                border: "1px solid rgba(189, 158, 255, 0.35)",
                color: "rgba(220, 210, 255, 0.95)",
              }}
            >
              {initialsOf(a)}
            </div>
          ))}
          {attendees.length > 8 && (
            <div
              className="w-7 h-7 rounded-full flex items-center justify-center text-[10px] font-mono"
              style={{ background: "rgba(255, 255, 255, 0.04)", border: "1px solid rgba(255, 255, 255, 0.12)", color: "rgba(236, 236, 241, 0.7)" }}
            >
              +{attendees.length - 8}
            </div>
          )}
        </div>
      )}

      {description && (
        <div className="px-4 py-3 text-[12.5px] leading-relaxed" style={{ color: "rgba(236, 236, 241, 0.82)", borderTop: "1px solid rgba(255, 255, 255, 0.06)" }}>
          {description}
        </div>
      )}

      {actions && actions.length > 0 && (
        <div className="px-4 py-3 flex flex-wrap gap-2" style={{ borderTop: "1px solid rgba(255, 255, 255, 0.06)" }}>
          {actions.map((a, i) => (
            <button
              key={i}
              onClick={() => setPendingComposerSubmit(a.verb)}
              className="px-3 py-1.5 rounded-md text-[12px]"
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
