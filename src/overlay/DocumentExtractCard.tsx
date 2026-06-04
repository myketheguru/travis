import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  type Document,
  extractDocument,
  formatBytes,
  getDocument,
  previewDocument,
  updateDocumentExtraction,
} from "../lib/documents";

interface Props {
  documentId: number;
  /// Called when the user accepts / dismisses the card. Card hides itself.
  onClose: () => void;
}

/// Renders a confirmation card showing what Travis extracted from a
/// document. Each field is editable inline; Save dispatches the
/// extraction overwrite. "Re-extract" forces a fresh extractor run.
/// "View source" opens the original PDF in the OS default viewer.
export function DocumentExtractCard({ documentId, onClose }: Props) {
  const [doc, setDoc] = useState<Document | null>(null);
  const [editing, setEditing] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const d = await getDocument(documentId);
      setDoc(d);
      setError(null);
    } catch (e) {
      setError((e as Error).message ?? String(e));
    }
  }, [documentId]);

  useEffect(() => {
    load();
  }, [load]);

  // Re-load whenever the backend fires the extraction event for THIS doc.
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    (async () => {
      try {
        unlisten = await listen<number>("document-extracted", (event) => {
          if (event.payload === documentId) {
            load();
          }
        });
      } catch {
        /* ignore */
      }
    })();
    return () => {
      try {
        unlisten?.();
      } catch {
        /* ignore */
      }
    };
  }, [documentId, load]);

  const extracted = (() => {
    if (!doc?.extractedJson) return null;
    try {
      return JSON.parse(doc.extractedJson) as Record<string, unknown>;
    } catch {
      return null;
    }
  })();

  const handleEdit = (path: string, value: string) => {
    setEditing((prev) => ({ ...prev, [path]: value }));
  };

  const handleSave = async () => {
    if (!doc || !extracted || Object.keys(editing).length === 0) return;
    setSaving(true);
    setError(null);
    try {
      // Merge edits into a fresh copy of the payload via dot-path.
      const next = structuredClone(extracted);
      for (const [path, raw] of Object.entries(editing)) {
        setDotPath(next, path, coerce(raw));
      }
      await updateDocumentExtraction(documentId, next);
      setEditing({});
      await load();
    } catch (e) {
      setError((e as Error).message ?? String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleReextract = async () => {
    setSaving(true);
    setError(null);
    try {
      await extractDocument(documentId, true);
      await load();
    } catch (e) {
      setError((e as Error).message ?? String(e));
    } finally {
      setSaving(false);
    }
  };

  if (!doc) {
    return (
      <div className="text-bone-3 text-[12px] px-4 py-3 font-mono">
        Loading doc#{documentId}…
      </div>
    );
  }

  const statusLabel = (() => {
    switch (doc.ingestStatus) {
      case "pending":
        return { text: "Reading…", color: "text-bone-3" };
      case "extracted":
        return { text: "Read", color: "text-pulse-2" };
      case "failed":
        return { text: "Read failed", color: "text-warn" };
      case "skipped":
        return { text: "Skipped", color: "text-bone-3" };
      default:
        return { text: doc.ingestStatus, color: "text-bone-3" };
    }
  })();

  return (
    <motion.div
      key={`extract-card-${documentId}`}
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -6 }}
      transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
      className="rounded-lg px-4 py-3"
      style={{
        background: "rgba(124, 92, 255, 0.06)",
        border: "1px solid rgba(124, 92, 255, 0.18)",
      }}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="text-bone text-[13px] font-medium truncate">
            <span className="text-pulse mr-1.5">◈</span>
            {doc.displayName}
          </div>
          <div className="text-bone-3 text-[11px] mt-0.5 font-mono">
            doc#{doc.id} · {doc.kind} · {formatBytes(doc.sizeBytes)} ·{" "}
            <span className={statusLabel.color}>{statusLabel.text}</span>
          </div>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <button
            onClick={() => previewDocument(documentId).catch(() => {})}
            className="text-[11px] text-bone-2 hover:text-bone px-2 py-1 rounded hover:bg-pulse/10"
            title="Open the source file in your default viewer"
            data-no-drag
          >
            view source
          </button>
          <button
            onClick={handleReextract}
            disabled={saving}
            className="text-[11px] text-bone-2 hover:text-bone px-2 py-1 rounded hover:bg-pulse/10 disabled:opacity-50"
            title="Re-run extraction from scratch"
            data-no-drag
          >
            re-extract
          </button>
          <button
            onClick={onClose}
            className="text-[11px] text-bone-3 hover:text-bone-2 px-2 py-1"
            title="Close this card"
            data-no-drag
          >
            ×
          </button>
        </div>
      </div>

      {doc.extractionError && (
        <div className="mt-3 text-[11px] text-warn/80 font-mono leading-relaxed">
          {doc.extractionError}
        </div>
      )}

      {extracted && (
        <div className="mt-3 space-y-1.5">
          {renderFieldList("", extracted, editing, handleEdit, 0)}
        </div>
      )}

      {extracted && Object.keys(editing).length > 0 && (
        <div className="mt-3 flex items-center justify-end gap-2">
          <button
            onClick={() => setEditing({})}
            className="text-[11px] text-bone-3 hover:text-bone-2 px-2 py-1"
            data-no-drag
          >
            cancel
          </button>
          <button
            onClick={handleSave}
            disabled={saving}
            className="text-[11px] text-bone font-medium px-3 py-1 rounded bg-pulse/20 hover:bg-pulse/30 disabled:opacity-50"
            data-no-drag
          >
            {saving
              ? "saving…"
              : `save ${Object.keys(editing).length} change${
                  Object.keys(editing).length === 1 ? "" : "s"
                }`}
          </button>
        </div>
      )}

      <AnimatePresence>
        {error && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="mt-2 text-[11px] text-warn font-mono"
          >
            {error}
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
}

/// Render extracted JSON as a tree of editable rows. Two-level nesting
/// is plenty for LTE doc shapes — line_items[].description etc.
function renderFieldList(
  prefix: string,
  value: unknown,
  editing: Record<string, string>,
  onEdit: (path: string, value: string) => void,
  depth: number,
): React.ReactNode {
  if (depth > 3) return null;
  if (Array.isArray(value)) {
    return value.map((item, i) => {
      const subPath = `${prefix}.${i}`;
      if (item !== null && typeof item === "object") {
        return (
          <div key={subPath} className="ml-3 mt-2">
            <div className="text-bone-3 text-[10px] font-mono uppercase tracking-wider mb-1">
              {prefix.split(".").pop()} #{i + 1}
            </div>
            <div className="ml-3 space-y-1.5">
              {renderFieldList(
                subPath,
                item,
                editing,
                onEdit,
                depth + 1,
              )}
            </div>
          </div>
        );
      }
      return (
        <FieldRow
          key={subPath}
          path={subPath.replace(/^\./, "")}
          label={`#${i + 1}`}
          value={item}
          editing={editing}
          onEdit={onEdit}
        />
      );
    });
  }
  if (value !== null && typeof value === "object") {
    return Object.entries(value as Record<string, unknown>).map(([k, v]) => {
      const subPath = prefix ? `${prefix}.${k}` : k;
      if (v !== null && typeof v === "object") {
        return (
          <div key={subPath} className="mt-2">
            <div className="text-bone-3 text-[10px] font-mono uppercase tracking-wider mb-1">
              {humanize(k)}
            </div>
            <div className="ml-3 space-y-1.5">
              {renderFieldList(subPath, v, editing, onEdit, depth + 1)}
            </div>
          </div>
        );
      }
      return (
        <FieldRow
          key={subPath}
          path={subPath.replace(/^\./, "")}
          label={humanize(k)}
          value={v}
          editing={editing}
          onEdit={onEdit}
        />
      );
    });
  }
  return null;
}

interface FieldRowProps {
  path: string;
  label: string;
  value: unknown;
  editing: Record<string, string>;
  onEdit: (path: string, value: string) => void;
}

function FieldRow({ path, label, value, editing, onEdit }: FieldRowProps) {
  const display = value === null || value === undefined ? "—" : String(value);
  const isEditing = path in editing;
  const current = isEditing ? editing[path] : display;
  const isEmpty = display === "—";

  return (
    <div className="flex items-baseline gap-3 text-[12px] group">
      <div className="text-bone-3 w-32 shrink-0 truncate" title={label}>
        {label}
      </div>
      <input
        type="text"
        value={current}
        onChange={(e) => onEdit(path, e.target.value)}
        className={`flex-1 bg-transparent border-b border-transparent hover:border-bone-3/30 focus:border-pulse focus:outline-none transition-colors px-1 py-0.5 ${
          isEmpty && !isEditing ? "text-bone-3 italic" : "text-bone"
        } ${isEditing ? "text-pulse-2" : ""}`}
        data-no-drag
      />
    </div>
  );
}

function humanize(slug: string): string {
  return slug
    .replace(/_/g, " ")
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .toLowerCase();
}

/// Coerce a string input back into a JSON-friendly value. Pure-numeric
/// strings become numbers; "true"/"false" become booleans; "" becomes
/// null; everything else stays a string.
function coerce(raw: string): unknown {
  const t = raw.trim();
  if (t === "") return null;
  if (t === "true") return true;
  if (t === "false") return false;
  if (/^-?\d+$/.test(t)) {
    const n = Number(t);
    if (Number.isSafeInteger(n)) return n;
  }
  if (/^-?\d+\.\d+$/.test(t)) {
    return Number(t);
  }
  return raw;
}

/// Set value at dot-path inside the given object, creating intermediate
/// containers as needed. Numeric segments target array indices.
function setDotPath(root: unknown, path: string, value: unknown): void {
  const segments = path.split(".").filter(Boolean);
  if (segments.length === 0) return;
  let current = root as Record<string, unknown> | unknown[];
  for (let i = 0; i < segments.length; i++) {
    const seg = segments[i];
    const isLast = i === segments.length - 1;
    const idx = Number.isInteger(Number(seg)) ? Number(seg) : null;
    if (isLast) {
      if (Array.isArray(current) && idx !== null) {
        current[idx] = value;
      } else if (typeof current === "object" && current !== null) {
        (current as Record<string, unknown>)[seg] = value;
      }
      return;
    }
    if (idx !== null && Array.isArray(current)) {
      current = current[idx] as Record<string, unknown> | unknown[];
    } else if (typeof current === "object" && current !== null) {
      current = (current as Record<string, unknown>)[seg] as
        | Record<string, unknown>
        | unknown[];
    }
    if (current === null || current === undefined) return;
  }
}
