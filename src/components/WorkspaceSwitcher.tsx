import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  getActiveWorkspace,
  isSensitive,
  listWorkspaces,
  setActiveWorkspaceCmd,
  type ActiveWorkspaceInfo,
  type Workspace,
} from "../lib/workspaces";

/// Top-of-window workspace switcher. Shows the active workspace
/// name; click to drop down a list of non-archived workspaces.
/// Picks the new active via set_active_workspace; the
/// `workspace-changed` event refreshes both this and any other
/// view that depends on workspace context.
export function WorkspaceSwitcher() {
  const [info, setInfo] = useState<ActiveWorkspaceInfo | null>(null);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    try {
      const [activeInfo, all] = await Promise.all([
        getActiveWorkspace(),
        listWorkspaces(),
      ]);
      setInfo(activeInfo);
      setWorkspaces(all.filter((w) => !w.archivedAt));
    } catch {
      /* swallow — keeps the switcher functional during transient errors */
    }
  }, []);

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

  // Close dropdown on outside-click.
  useEffect(() => {
    if (!open) return;
    const onClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onClick);
    return () => document.removeEventListener("mousedown", onClick);
  }, [open]);

  const pick = async (id: number) => {
    setOpen(false);
    if (!info || info.workspace.id === id) return;
    try {
      await setActiveWorkspaceCmd(id);
    } catch {
      /* error surfacing comes via the toast layer when that lands */
    }
  };

  if (!info) return null;

  const active = info.workspace;
  const activeIsSensitive = isSensitive(active.category);

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        className={
          "flex items-center gap-1.5 px-2.5 py-1.5 rounded-full text-xs transition-colors " +
          (activeIsSensitive
            ? "border border-warn/30 bg-warn/[0.07] text-warn hover:bg-warn/[0.12]"
            : "border border-ink-3 bg-ink-2/30 text-bone-2 hover:bg-ink-2/50")
        }
        title={`Active workspace: ${active.name}${activeIsSensitive ? " (sensitive — isolated from others)" : ""}`}
      >
        {activeIsSensitive && <span aria-hidden>🔒</span>}
        <span className="font-medium">{active.name}</span>
        <span className="opacity-50 text-[10px]" aria-hidden>
          {open ? "▴" : "▾"}
        </span>
      </button>

      {open && (
        <div className="absolute right-0 top-full mt-2 z-30 min-w-[200px] rounded-xl border border-ink-3 bg-ink/95 backdrop-blur-sm shadow-lg overflow-hidden">
          <div className="text-[10px] tracking-[0.18em] uppercase text-bone-3 px-3 pt-3 pb-1">
            Workspaces
          </div>
          {workspaces.map((w) => {
            const sensitive = isSensitive(w.category);
            const isActive = w.id === active.id;
            return (
              <button
                key={w.id}
                onClick={() => pick(w.id)}
                className={
                  "w-full flex items-center gap-2 px-3 py-2 text-sm text-left transition-colors " +
                  (isActive
                    ? "bg-pulse/[0.10] text-bone"
                    : "text-bone-2 hover:bg-white/[0.04]")
                }
              >
                {sensitive && (
                  <span className="text-warn text-[10px]" aria-hidden>
                    🔒
                  </span>
                )}
                <span className="flex-1 truncate">{w.name}</span>
                <span className="text-[10px] tracking-wider text-bone-3 uppercase opacity-70">
                  {w.category}
                </span>
                {isActive && (
                  <span
                    className="h-1.5 w-1.5 rounded-full bg-pulse-2"
                    aria-hidden
                  />
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
