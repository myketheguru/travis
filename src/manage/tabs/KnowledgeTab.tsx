import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  listEntitiesByFamily,
  type EntityListRow,
  type KnowledgeFamily,
} from "../../lib/knowledge";

const HEADERS: Record<KnowledgeFamily, { title: string; blurb: string }> = {
  person: {
    title: "People",
    blurb:
      "Coaches, tutors, students, friends — every person Travis has heard you mention. Ambient mentions sit at lower confidence; pack-projected entities are exact records.",
  },
  place: {
    title: "Places",
    blurb:
      "Schools, offices, locations. Anywhere named in a journal entry that's worth keeping a memory of.",
  },
  org: {
    title: "Organisations",
    blurb:
      "Departments, agencies, vendors, companies. Anything organisational the user mentions in passing or refers to repeatedly.",
  },
};

const KIND_LABELS: Record<string, string> = {
  "person:unknown": "Person (unclassified)",
  "person:coach": "Coach (refined)",
  "person:friend": "Friend",
  "place:unknown": "Place (unclassified)",
  "org:unknown": "Org (unclassified)",
};

function formatKind(kind: string): string {
  return KIND_LABELS[kind] ?? kind;
}

function formatLastSeen(ts: string): string {
  const date = ts.split("T")[0]?.split(" ")[0] ?? ts;
  return date;
}

function confidenceLabel(confidence: number): string {
  if (confidence >= 0.95) return "certain";
  if (confidence >= 0.7) return "likely";
  if (confidence >= 0.5) return "ambient";
  return "unsure";
}

/// Cross-pack entity list. One component renders all three Knowledge
/// tabs (People / Places / Orgs) — they share schema, only the family
/// filter differs. Refreshes on workspace-changed so switching the
/// active workspace re-scopes the list immediately.
export default function KnowledgeTab({ family }: { family: KnowledgeFamily }) {
  const [rows, setRows] = useState<EntityListRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const r = await listEntitiesByFamily(family);
      setRows(r);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [family]);

  useEffect(() => {
    refresh();
    let unlistenFn: (() => void) | null = null;
    listen("workspace-changed", () => refresh()).then((fn) => {
      unlistenFn = fn;
    });
    return () => {
      if (unlistenFn) unlistenFn();
    };
  }, [refresh]);

  const head = HEADERS[family];

  return (
    <div className="px-10 py-6 max-w-5xl mx-auto">
      <div className="flex items-baseline justify-between mb-4">
        <div>
          <h2 className="text-bone text-lg font-light tracking-tight">{head.title}</h2>
          <p className="text-bone-3 text-[11px] mt-1 max-w-xl leading-relaxed">
            {head.blurb}
          </p>
        </div>
        <span className="text-bone-3 text-[10px] font-mono">{rows.length} known</span>
      </div>

      {loading && (
        <p className="text-bone-3 text-xs">Loading…</p>
      )}

      {error && !loading && (
        <p className="text-warn text-xs">Couldn't load: {error}</p>
      )}

      {!loading && !error && rows.length === 0 && (
        <p className="text-bone-3 text-xs leading-relaxed max-w-xl">
          Nothing yet. Travis records {head.title.toLowerCase()} as you mention
          them in journal captures — try Cmd+J and write a note that names someone
          or somewhere, then come back here.
        </p>
      )}

      {!loading && !error && rows.length > 0 && (
        <div className="rounded-xl border border-ink-3 bg-ink-2/30 overflow-hidden">
          <div className="grid grid-cols-[1fr_140px_100px_120px] gap-3 px-4 py-2.5 text-[10px] tracking-wider uppercase text-bone-3 border-b border-white/[0.04]">
            <span>Name</span>
            <span>Kind</span>
            <span className="text-right">Mentions</span>
            <span>Last seen</span>
          </div>
          {rows.map((r) => (
            <div
              key={r.id}
              className="grid grid-cols-[1fr_140px_100px_120px] gap-3 px-4 py-2.5 text-sm text-bone-2 hover:bg-white/[0.02] border-b border-white/[0.03] last:border-b-0"
              title={`Confidence: ${r.confidence.toFixed(2)} (${confidenceLabel(r.confidence)})`}
            >
              <span className="flex items-center gap-2 truncate">
                <span
                  className={
                    "h-1.5 w-1.5 rounded-full shrink-0 " +
                    (r.confidence >= 0.95
                      ? "bg-pulse-2"
                      : r.confidence >= 0.7
                      ? "bg-pulse/70"
                      : "bg-bone-3/50")
                  }
                  aria-hidden
                />
                <span className="truncate">{r.displayName}</span>
              </span>
              <span className="text-[10px] tracking-wider uppercase text-bone-3 self-center truncate">
                {formatKind(r.kind)}
              </span>
              <span className="text-right text-bone-3 self-center font-mono text-[11px]">
                {r.mentionsCount}
              </span>
              <span className="text-bone-3 self-center text-[11px] font-mono">
                {formatLastSeen(r.lastSeen)}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
