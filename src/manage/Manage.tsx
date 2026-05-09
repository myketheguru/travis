import { useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import AskTab from "./tabs/AskTab";
import TasksTab from "./tabs/TasksTab";
import RemindersTab from "./tabs/RemindersTab";
import EntitiesTab from "./tabs/EntitiesTab";
import SummariesTab from "./tabs/SummariesTab";
import AsksTab from "./tabs/AsksTab";
import ThreadsTab from "./tabs/ThreadsTab";
import { useAppStore } from "../stores/app";
import { packSchemas, type PackSchema, type TableDef } from "../lib/packs";
import { TableTab } from "../lib/autoCRUD";
import { getOverride } from "../lib/packRegistry";

/// Top-level tab identifier. Core tabs use a string id; pack tabs
/// use the namespaced form `pack:<slug>:<table>`.
type CoreTabId =
  | "ask"
  | "threads"
  | "tasks"
  | "reminders"
  | "entities"
  | "summaries"
  | "asks";

type CoreTab = {
  kind: "core";
  id: CoreTabId;
  label: string;
  diagnostic?: boolean;
};

type PackTab = {
  kind: "pack";
  id: string; // pack:<packSlug>:<tableSlug>
  label: string;
  pack: PackSchema;
  table: TableDef;
};

type Tab = CoreTab | PackTab;

/// One sidebar group. The label is shown as a small uppercase
/// header above the items; items render in declaration order.
type Group = {
  label: string;
  /// When true, the whole group is hidden unless `showDiagnostics`
  /// is on. Used to keep the dev/inspection tabs out of the way.
  diagnostic?: boolean;
  items: Tab[];
};

const captureTabs: CoreTab[] = [
  { kind: "core", id: "ask",       label: "Ask" },
  { kind: "core", id: "tasks",     label: "Tasks" },
  { kind: "core", id: "threads",   label: "Threads" },
  { kind: "core", id: "reminders", label: "Reminders" },
];

const diagnosticTabs: CoreTab[] = [
  { kind: "core", id: "entities",  label: "Entities",   diagnostic: true },
  { kind: "core", id: "summaries", label: "Summaries",  diagnostic: true },
  { kind: "core", id: "asks",      label: "Asks of me", diagnostic: true },
];

export default function Manage({ onClose }: { onClose: () => void }) {
  const showDiagnostics = useAppStore((s) => s.showDiagnostics);
  const enabledPacks = useAppStore((s) => s.status?.enabledPacks ?? []);
  const [schemas, setSchemas] = useState<PackSchema[] | null>(null);
  const [tab, setTab] = useState<string>("ask");

  useEffect(() => {
    packSchemas().then(setSchemas).catch(() => setSchemas([]));
  }, []);

  // Group tabs by section. Pack tabs become one group per pack —
  // `pack.name` is the user-facing display name, e.g. "Lead to
  // Empower". Diagnostics is the trailing collapsible group, only
  // visible when the user has the toggle on.
  const groups = useMemo<Group[]>(() => {
    const out: Group[] = [
      { label: "Capture", items: captureTabs },
    ];
    for (const pack of schemas ?? []) {
      const items: PackTab[] = pack.tables
        .filter((t) => t.primary)
        .map<PackTab>((t) => ({
          kind: "pack",
          id: `pack:${pack.slug}:${t.slug}`,
          label: t.displayName,
          pack,
          table: t,
        }));
      if (items.length > 0) {
        out.push({ label: pack.name, items });
      }
    }
    out.push({ label: "Diagnostics", diagnostic: true, items: diagnosticTabs });
    return out;
  }, [schemas]);
  void enabledPacks; // see prior comment

  // Flat list of currently-visible tabs, used for the active-fallback
  // logic and the active lookup.
  const visibleTabs = useMemo<Tab[]>(() => {
    return groups
      .filter((g) => !g.diagnostic || showDiagnostics)
      .flatMap((g) => g.items);
  }, [groups, showDiagnostics]);

  useEffect(() => {
    if (!visibleTabs.find((t) => t.id === tab)) {
      setTab("ask");
    }
  }, [visibleTabs, tab]);

  const active = visibleTabs.find((t) => t.id === tab);

  return (
    <main className="relative h-full w-full overflow-hidden flex">
      {/* Sidebar */}
      <aside className="w-56 shrink-0 border-r border-white/[0.05] bg-ink-2/20 flex flex-col">
        <div className="px-5 pt-6 pb-3">
          <button
            onClick={onClose}
            className="text-bone-3 hover:text-bone-2 text-xs flex items-center gap-1.5 transition-colors mb-4"
          >
            <span aria-hidden>←</span>
            <span>Back</span>
          </button>
          <h1 className="text-bone text-base font-light tracking-tight">Manage</h1>
        </div>

        <nav className="flex-1 overflow-y-auto pb-6">
          {groups.map((g) => {
            if (g.diagnostic && !showDiagnostics) return null;
            return (
              <div key={g.label} className="mt-4 first:mt-2 px-3">
                <div className="px-2 pb-1.5 text-[10px] tracking-[0.2em] uppercase text-bone-3/60">
                  {g.label}
                </div>
                <div className="flex flex-col">
                  {g.items.map((t) => {
                    const isActive = tab === t.id;
                    return (
                      <button
                        key={t.id}
                        onClick={() => setTab(t.id)}
                        className={
                          "relative text-left px-3 py-1.5 rounded-md text-[13px] transition-colors " +
                          (isActive
                            ? "text-bone bg-pulse/[0.08]"
                            : "text-bone-2 hover:text-bone hover:bg-white/[0.025]")
                        }
                      >
                        {isActive && (
                          <motion.span
                            layoutId="manage-active-accent"
                            className="absolute left-0 top-1.5 bottom-1.5 w-[2px] rounded-r-full bg-pulse"
                            transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
                          />
                        )}
                        <span className="block truncate pl-1.5">{t.label}</span>
                      </button>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </nav>
      </aside>

      {/* Content */}
      <section className="flex-1 overflow-y-auto">
        {active?.kind === "core" && active.id === "ask" && <AskTab />}
        {active?.kind === "core" && active.id === "threads" && <ThreadsTab />}
        {active?.kind === "core" && active.id === "tasks" && <TasksTab />}
        {active?.kind === "core" && active.id === "reminders" && <RemindersTab />}
        {active?.kind === "core" && active.id === "entities" && <EntitiesTab />}
        {active?.kind === "core" && active.id === "summaries" && <SummariesTab />}
        {active?.kind === "core" && active.id === "asks" && <AsksTab />}
        {active?.kind === "pack" && (() => {
          // Pack-shipped custom UI takes priority over auto-CRUD when
          // an override is declared. PLUGIN_PLATFORM.md explains the
          // override mechanism + how it'll evolve when runtime-loaded
          // packs land.
          const Override = getOverride(active.pack.slug, active.table.slug, "list");
          if (Override) {
            return <Override />;
          }
          // Key forces a fresh TableTab (and its ListView) every time
          // the user switches tables — otherwise React preserves the
          // previous tab's sortField state, leaking e.g. "name" from
          // Coaches into the Hours tab whose fields don't include it.
          return (
            <TableTab
              key={`${active.pack.slug}:${active.table.slug}`}
              pack={active.pack}
              table={active.table}
            />
          );
        })()}
      </section>
    </main>
  );
}
