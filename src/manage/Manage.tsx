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

const coreTabsBefore: CoreTab[] = [
  { kind: "core", id: "ask",       label: "Ask" },
  { kind: "core", id: "threads",   label: "Threads" },
  { kind: "core", id: "tasks",     label: "Tasks" },
];

const coreTabsAfter: CoreTab[] = [
  { kind: "core", id: "reminders", label: "Reminders" },
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

  const tabs = useMemo<Tab[]>(() => {
    // Every primary pack table becomes a tab. The packRegistry decides
    // whether the tab body is rendered by an auto-CRUD component or a
    // pack-shipped custom override; both paths look the same here.
    const packTabs: PackTab[] = (schemas ?? []).flatMap((pack) =>
      pack.tables
        .filter((t) => t.primary)
        .map<PackTab>((t) => ({
          kind: "pack",
          id: `pack:${pack.slug}:${t.slug}`,
          label: t.displayName,
          pack,
          table: t,
        })),
    );

    const all: Tab[] = [...coreTabsBefore, ...packTabs, ...coreTabsAfter];
    return all.filter((t) => {
      if (t.kind === "core" && t.diagnostic && !showDiagnostics) {
        return false;
      }
      return true;
    });
  }, [schemas, showDiagnostics]);
  // Suppress the lint about enabledPacks no longer being a dep — it's
  // intentionally not used now that pack tabs come from packSchemas
  // (which is already enabled-only).
  void enabledPacks;

  // If the active tab disappears (diagnostics toggled off, pack disabled),
  // fall back to Ask.
  useEffect(() => {
    if (!tabs.find((t) => t.id === tab)) {
      setTab("ask");
    }
  }, [tabs, tab]);

  const active = tabs.find((t) => t.id === tab);

  return (
    <main className="relative h-full w-full overflow-hidden flex flex-col">
      <button
        onClick={onClose}
        className="absolute top-6 left-6 text-bone-3 hover:text-bone-2 text-xs flex items-center gap-1.5 transition-colors z-10"
      >
        <span aria-hidden>←</span>
        <span>Back</span>
      </button>

      <motion.div
        className="pt-16 pb-3 px-10"
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.35, ease: [0.16, 1, 0.3, 1] }}
      >
        <h1 className="text-2xl font-light tracking-tight text-bone">Manage</h1>
        <p className="text-bone-3 text-xs mt-1">Browse, query, and act on what Travis knows.</p>
      </motion.div>

      <div className="px-10 border-b border-white/[0.05] flex items-center gap-1 overflow-x-auto">
        {tabs.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            className={
              "relative px-3 py-2.5 text-xs tracking-wider transition-colors whitespace-nowrap " +
              (tab === t.id ? "text-bone" : "text-bone-3 hover:text-bone-2")
            }
          >
            {t.label}
            {tab === t.id && (
              <motion.span
                layoutId="manage-tab-underline"
                className="absolute left-3 right-3 -bottom-px h-[2px] rounded-full bg-pulse"
                transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
              />
            )}
          </button>
        ))}
      </div>

      <div className="flex-1 overflow-y-auto">
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
          return <TableTab pack={active.pack} table={active.table} />;
        })()}
      </div>
    </main>
  );
}
