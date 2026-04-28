import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import AskTab from "./tabs/AskTab";
import TasksTab from "./tabs/TasksTab";
import RemindersTab from "./tabs/RemindersTab";
import EntitiesTab from "./tabs/EntitiesTab";
import SummariesTab from "./tabs/SummariesTab";
import AsksTab from "./tabs/AsksTab";
import ThreadsTab from "./tabs/ThreadsTab";
import InvoicesTab from "./tabs/InvoicesTab";
import { useAppStore } from "../stores/app";

type Tab = "ask" | "threads" | "tasks" | "invoices" | "reminders" | "entities" | "summaries" | "asks";

type TabDef = { id: Tab; label: string; diagnostic?: boolean };

const allTabs: TabDef[] = [
  { id: "ask",       label: "Ask" },
  { id: "threads",   label: "Threads" },
  { id: "tasks",     label: "Tasks" },
  { id: "invoices",  label: "Invoices" },
  { id: "reminders", label: "Reminders" },
  { id: "entities",  label: "Entities",   diagnostic: true },
  { id: "summaries", label: "Summaries",  diagnostic: true },
  { id: "asks",      label: "Asks of me", diagnostic: true },
];

export default function Manage({ onClose }: { onClose: () => void }) {
  const showDiagnostics = useAppStore((s) => s.showDiagnostics);
  const tabs = allTabs.filter((t) => !t.diagnostic || showDiagnostics);
  const [tab, setTab] = useState<Tab>("ask");

  // If the active tab gets hidden by toggling diagnostics off, fall back to Ask.
  useEffect(() => {
    if (!tabs.find((t) => t.id === tab)) {
      setTab("ask");
    }
  }, [tabs, tab]);

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

      <div className="px-10 border-b border-white/[0.05] flex items-center gap-1">
        {tabs.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            className={
              "relative px-3 py-2.5 text-xs tracking-wider transition-colors " +
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
        {tab === "ask" && <AskTab />}
        {tab === "threads" && <ThreadsTab />}
        {tab === "tasks" && <TasksTab />}
        {tab === "invoices" && <InvoicesTab />}
        {tab === "reminders" && <RemindersTab />}
        {tab === "entities" && <EntitiesTab />}
        {tab === "summaries" && <SummariesTab />}
        {tab === "asks" && <AsksTab />}
      </div>
    </main>
  );
}
