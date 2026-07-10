/**
 * RouteStepsCard — v0.28.29 Phase B.
 *
 * Turn-by-turn directions. Renders alongside a MapPart to give the
 * user the segment breakdown for a route.
 */
interface Props {
  from_label?: string;
  to_label?: string;
  total_distance_meters?: number;
  total_duration_seconds?: number;
  profile?: "driving-car" | "cycling-regular" | "foot-walking";
  steps: {
    instruction: string;
    distance_meters?: number;
    duration_seconds?: number;
    street?: string;
  }[];
  narration?: string;
}

function fmtDistance(m?: number): string {
  if (typeof m !== "number") return "";
  if (m < 1000) return `${Math.round(m)} m`;
  return `${(m / 1000).toFixed(1)} km`;
}
function fmtDuration(s?: number): string {
  if (typeof s !== "number") return "";
  const min = Math.round(s / 60);
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  const rem = min - h * 60;
  return rem === 0 ? `${h} h` : `${h} h ${rem} min`;
}

export function RouteStepsCard({ from_label, to_label, total_distance_meters, total_duration_seconds, profile, steps, narration }: Props) {
  return (
    <div
      className="rounded-2xl overflow-hidden"
      style={{
        border: "1px solid rgba(189, 158, 255, 0.28)",
        background: "linear-gradient(180deg, rgba(28, 24, 40, 0.55), rgba(20, 18, 30, 0.52))",
      }}
    >
      <div className="px-4 pt-3 pb-2" style={{ borderBottom: "1px solid rgba(189, 158, 255, 0.18)" }}>
        <div className="text-[10.5px] uppercase tracking-[0.22em] font-mono mb-1" style={{ color: "rgba(189, 158, 255, 0.85)" }}>
          {profile ? profile.replace("-", " ") : "route"}
        </div>
        {(from_label || to_label) && (
          <div className="text-[13.5px] leading-snug" style={{ color: "rgb(240, 240, 246)" }}>
            {from_label ?? "—"} <span style={{ color: "rgba(236, 236, 241, 0.5)" }}>→</span> {to_label ?? "—"}
          </div>
        )}
        {(total_distance_meters || total_duration_seconds) && (
          <div className="text-[12px] font-mono mt-1" style={{ color: "rgba(236, 236, 241, 0.7)" }}>
            {[fmtDuration(total_duration_seconds), fmtDistance(total_distance_meters)].filter(Boolean).join(" · ")}
          </div>
        )}
      </div>

      <ol className="px-4 py-2.5">
        {steps.map((s, i) => (
          <li
            key={i}
            className="flex items-start gap-3 py-1.5"
            style={{ borderBottom: i < steps.length - 1 ? "1px solid rgba(255, 255, 255, 0.04)" : "none" }}
          >
            <div
              className="shrink-0 w-6 h-6 mt-0.5 rounded-full flex items-center justify-center text-[11px] font-mono"
              style={{
                background: "rgba(189, 158, 255, 0.15)",
                border: "1px solid rgba(189, 158, 255, 0.35)",
                color: "rgba(220, 210, 255, 0.95)",
              }}
            >
              {i + 1}
            </div>
            <div className="min-w-0 flex-1">
              <div className="text-[13px] leading-snug" style={{ color: "rgba(236, 236, 241, 0.94)" }}>
                {s.instruction}
              </div>
              {(s.street || s.distance_meters || s.duration_seconds) && (
                <div className="text-[11.5px] font-mono mt-0.5" style={{ color: "rgba(236, 236, 241, 0.55)" }}>
                  {[s.street, fmtDistance(s.distance_meters), fmtDuration(s.duration_seconds)].filter(Boolean).join(" · ")}
                </div>
              )}
            </div>
          </li>
        ))}
      </ol>

      {narration && (
        <div className="px-4 py-2 text-[12px]" style={{ color: "rgba(236, 236, 241, 0.72)", borderTop: "1px solid rgba(255, 255, 255, 0.06)" }}>
          {narration}
        </div>
      )}
    </div>
  );
}
