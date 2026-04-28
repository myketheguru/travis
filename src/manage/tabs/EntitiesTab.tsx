import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type EntityIndex = {
  id: number;
  kind: string;
  normalizedName: string;
  displayName: string;
  mentionsCount: number;
  firstSeen: string;
  lastSeen: string;
  attributesJson: string | null;
};

const groups: { kind: string; label: string }[] = [
  { kind: "coach", label: "Coaches" },
  { kind: "school", label: "Schools" },
  { kind: "dept", label: "Departments" },
];

export default function EntitiesTab() {
  const [items, setItems] = useState<EntityIndex[]>([]);
  const [loading, setLoading] = useState(true);
  const [blurb, setBlurb] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const list = await invoke<EntityIndex[]>("list_entities", { limit: 100 });
      setItems(list);
      const b = await invoke<string>("get_profile_blurb");
      setBlurb(b);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  if (loading) {
    return (
      <div className="px-10 py-6 max-w-3xl mx-auto">
        <p className="text-bone-3 text-xs">Loading…</p>
      </div>
    );
  }

  return (
    <div className="px-10 py-6 max-w-3xl mx-auto flex flex-col gap-7">
      {blurb && (
        <div className="rounded-xl border border-ink-3 bg-ink-2/30 p-4">
          <div className="text-bone-3 text-[10px] tracking-[0.18em] uppercase mb-2">
            Profile blurb (injected into LLM context)
          </div>
          <p className="text-bone-2 text-xs leading-relaxed">{blurb}</p>
        </div>
      )}

      {groups.map((g) => {
        const rows = items.filter((i) => i.kind === g.kind);
        return (
          <section key={g.kind}>
            <div className="flex items-center justify-between mb-2">
              <h3 className="text-bone text-sm font-medium">{g.label}</h3>
              <span className="text-bone-3 text-[10px] font-mono">{rows.length}</span>
            </div>
            {rows.length === 0 ? (
              <p className="text-bone-3 text-xs">None mentioned yet.</p>
            ) : (
              <div className="flex flex-col">
                {rows.map((r) => (
                  <div
                    key={r.id}
                    className="flex items-center gap-3 py-2 border-b border-white/[0.03]"
                  >
                    <span className="text-bone-2 text-sm flex-1">{r.displayName}</span>
                    <span className="text-bone-3 text-[11px] font-mono">
                      {r.mentionsCount} mention{r.mentionsCount === 1 ? "" : "s"}
                    </span>
                    <span className="text-bone-3 text-[10px] font-mono opacity-70">
                      last {r.lastSeen.slice(0, 10)}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </section>
        );
      })}
    </div>
  );
}
