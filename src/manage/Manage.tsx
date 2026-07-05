import { useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { DocumentViewer } from "../chat/DocumentViewer";
import { ResizableSplit } from "../chat/ResizableSplit";
import { FloatingChatOverlay } from "../chat/FloatingChatOverlay";
import { Workspace } from "../workspace/Workspace";
import TasksTab from "./tabs/TasksTab";
import RemindersTab from "./tabs/RemindersTab";
import EntitiesTab from "./tabs/EntitiesTab";
import SummariesTab from "./tabs/SummariesTab";
import AsksTab from "./tabs/AsksTab";
import ThreadsTab from "./tabs/ThreadsTab";
import DocumentsTab from "./tabs/DocumentsTab";
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
  | "documents"
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
  { kind: "core", id: "documents", label: "Documents" },
];

const diagnosticTabs: CoreTab[] = [
  { kind: "core", id: "entities",  label: "Entities",   diagnostic: true },
  { kind: "core", id: "summaries", label: "Summaries",  diagnostic: true },
  { kind: "core", id: "asks",      label: "Asks of me", diagnostic: true },
];

export default function Manage({ onClose }: { onClose: () => void }) {
  const showDiagnostics = useAppStore((s) => s.showDiagnostics);
  const enabledPacks = useAppStore((s) => s.status?.enabledPacks ?? []);
  const viewerDocumentId = useAppStore((s) => s.viewerDocumentId);
  const chatPaneFraction = useAppStore((s) => s.chatPaneFraction);
  const setChatPaneFraction = useAppStore((s) => s.setChatPaneFraction);
  const docFullscreen = useAppStore((s) => s.docFullscreen);
  const [schemas, setSchemas] = useState<PackSchema[] | null>(null);
  const [tab, setTab] = useState<string>("ask");

  useEffect(() => {
    packSchemas().then(setSchemas).catch(() => setSchemas([]));
  }, []);

  // Group tabs by section. Pack tabs ONLY appear when the pack is
  // actually enabled for this user (via PacksSection toggle, or via
  // an Org entitlement flowing back from the cloud). A pack that
  // ships in the binary but isn't enabled for this user stays
  // hidden — sidebar is supposed to reflect the user's actual work
  // surfaces, not "every pack we shipped."
  //
  // Diagnostics is the trailing collapsible group, only visible when
  // the user has the toggle on.
  const enabledPackSet = useMemo(
    () => new Set(enabledPacks),
    [enabledPacks],
  );
  const groups = useMemo<Group[]>(() => {
    const out: Group[] = [
      { label: "Capture", items: captureTabs },
    ];
    for (const pack of schemas ?? []) {
      if (!enabledPackSet.has(pack.slug)) continue;
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
  }, [schemas, enabledPackSet]);

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

      {/*
        Content layout: when a doc is open AND docFullscreen is on, the
        chat pane is hidden and a floating overlay handles input. When a
        doc is open without fullscreen, we render the split layout. With
        no doc open, single-pane.
      */}
      {viewerDocumentId != null && docFullscreen ? (
        <>
          <section className="flex-1 h-full">
            <DocumentViewer documentId={viewerDocumentId} />
          </section>
          <FloatingChatOverlay />
        </>
      ) : viewerDocumentId != null ? (
        <ResizableSplit
          fraction={chatPaneFraction}
          onFractionChange={setChatPaneFraction}
          left={<section className="flex-1 overflow-y-auto h-full">{renderActiveContent(active, schemas)}</section>}
          right={<DocumentViewer documentId={viewerDocumentId} />}
        />
      ) : (
        <section className="flex-1 overflow-y-auto">{renderActiveContent(active, schemas)}</section>
      )}
    </main>
  );
}

function renderActiveContent(active: Tab | undefined, schemas: PackSchema[] | null): React.ReactNode {
  void schemas;
  if (!active) return null;
  if (active.kind === "core") {
    // v0.22.15 (Shell 8) — Ask tab now renders the Workspace shell,
    // which composes AttentionStrip + workspace controls above the
    // existing AskTab surface. AskTab still holds the primary chat
    // canvas; Workspace adds the peripheral chrome around it.
    if (active.id === "ask") return <Workspace />;
    if (active.id === "threads") return <ThreadsTab />;
    if (active.id === "tasks") return <TasksTab />;
    if (active.id === "reminders") return <RemindersTab />;
    if (active.id === "documents") return <DocumentsTab />;
    if (active.id === "entities") return <EntitiesTab />;
    if (active.id === "summaries") return <SummariesTab />;
    if (active.id === "asks") return <AsksTab />;
  }
  if (active.kind === "pack") {
    const Override = getOverride(active.pack.slug, active.table.slug, "list");
    if (Override) return <Override />;
    return (
      <TableTab
        key={`${active.pack.slug}:${active.table.slug}`}
        pack={active.pack}
        table={active.table}
      />
    );
  }
  return null;
}
