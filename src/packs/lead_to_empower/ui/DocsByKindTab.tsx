/**
 * v0.20.5 — PO / WO tab override.
 *
 * The typed `purchase_order` / `work_order` tables require engagement
 * FKs + a parsed set of fields the LLM doesn't reliably emit yet, so
 * those tables stay empty even after the user drops a PO/WO PDF in
 * chat. The auto-CRUD list view for those tables then shows "no rows"
 * which makes Travis look like it ignored the drop.
 *
 * As an interim fix: render `document` rows whose `kind` matches
 * (po / purchase_order or wo / work_order) directly in the PO/WO
 * tabs. Once typed extraction lands the auto-CRUD view can take over.
 */
import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import {
  downloadDocument,
  listDocuments,
  previewDocument,
  revealDocumentInFolder,
  type Document,
} from "../../../lib/documents";
import { DocumentIcon } from "../../../chat/DocumentIcon";
import { useAppStore } from "../../../stores/app";

function DocsByKindTabImpl({
  kinds,
  emptyHint,
}: {
  kinds: string[];
  emptyHint: string;
}) {
  const [docs, setDocs] = useState<Document[]>([]);
  const [loading, setLoading] = useState(true);
  const setViewerDocumentId = useAppStore((s) => s.setViewerDocumentId);

  useEffect(() => {
    let cancelled = false;
    listDocuments({ limit: 500 })
      .then((all) => {
        if (cancelled) return;
        const lower = new Set(kinds.map((k) => k.toLowerCase()));
        setDocs(all.filter((d) => lower.has((d.kind ?? "").toLowerCase())));
      })
      .catch(() => !cancelled && setDocs([]))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [kinds]);

  if (loading) {
    return <div className="p-8 text-bone-3 text-sm">Loading…</div>;
  }

  if (docs.length === 0) {
    return (
      <div className="p-10 max-w-xl mx-auto">
        <p className="text-bone-3 text-sm leading-relaxed">{emptyHint}</p>
      </div>
    );
  }

  return (
    <div className="p-6 max-w-3xl mx-auto flex flex-col gap-2">
      {docs.map((d) => (
        <motion.div
          key={d.id}
          initial={{ opacity: 0, y: 4 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.18 }}
          className="flex items-center gap-3 rounded-xl border border-ink-3 bg-ink-2/30 hover:bg-ink-2/50 px-4 py-3 transition-colors"
        >
          <span className="text-bone-2 shrink-0">
            <DocumentIcon kind={d.kind} mimeType={d.mimeType} size={18} />
          </span>
          <div className="flex-1 min-w-0">
            <div className="text-bone text-sm truncate">{d.displayName}</div>
            <div className="text-bone-3 text-[10px] font-mono">
              {d.kind} · {Math.round(d.sizeBytes / 1024)} KB · doc#{d.id}
            </div>
          </div>
          <div className="flex items-center gap-1.5">
            <button
              onClick={() => setViewerDocumentId(d.id)}
              className="text-bone-3 hover:text-bone-2 text-[11px] underline-offset-4 hover:underline"
            >
              preview
            </button>
            <button
              onClick={() => previewDocument(d.id).catch(() => {})}
              className="text-bone-3 hover:text-bone-2 text-[11px] underline-offset-4 hover:underline"
            >
              open
            </button>
            <button
              onClick={() => revealDocumentInFolder(d.id).catch(() => {})}
              className="text-bone-3 hover:text-bone-2 text-[11px] underline-offset-4 hover:underline"
            >
              reveal
            </button>
            <button
              onClick={() =>
                downloadDocument(d.id, d.originalFilename).catch(() => {})
              }
              className="text-bone-3 hover:text-bone-2 text-[11px] underline-offset-4 hover:underline"
            >
              download
            </button>
          </div>
        </motion.div>
      ))}
    </div>
  );
}

export function PurchaseOrdersTab() {
  return (
    <DocsByKindTabImpl
      kinds={["po", "purchase_order"]}
      emptyHint="No POs uploaded yet. Drop a PO PDF in chat or attach it from the Documents tab — Travis will classify it as a PO and it'll appear here."
    />
  );
}

export function WorkOrdersTab() {
  return (
    <DocsByKindTabImpl
      kinds={["wo", "work_order"]}
      emptyHint="No WOs uploaded yet. Drop a WO PDF in chat or attach it from the Documents tab — Travis will classify it as a WO and it'll appear here."
    />
  );
}
