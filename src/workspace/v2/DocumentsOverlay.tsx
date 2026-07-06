/**
 * DocumentsOverlay — v2 Shell 12b.
 *
 * Floating card listing recent documents. Click a doc to open it in the
 * DocumentViewer (via the app store's viewerDocumentId). Search filter
 * narrows by display name / original filename.
 *
 * The dock's Docs icon opens this. ⌘D also opens it. Esc closes.
 */
import { useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useAppStore } from "../../stores/app";
import { listDocuments, type Document } from "../../lib/documents";

export function DocumentsOverlay() {
  const open = useAppStore((s) => s.documentsOverlayOpen);
  const setOpen = useAppStore((s) => s.setDocumentsOverlayOpen);
  const setViewerDocumentId = useAppStore((s) => s.setViewerDocumentId);

  const [query, setQuery] = useState("");
  const [items, setItems] = useState<Document[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    listDocuments({ limit: 60 })
      .then((list) => {
        if (!cancelled) setItems(list);
      })
      .catch((e) => !cancelled && setError(String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setOpen]);

  function handleSelect(doc: Document) {
    setViewerDocumentId(doc.id);
    setOpen(false);
    setQuery("");
  }

  const q = query.trim().toLowerCase();
  const filtered = q
    ? items.filter(
        (d) =>
          d.displayName.toLowerCase().includes(q) ||
          d.originalFilename.toLowerCase().includes(q),
      )
    : items;

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          key="docs-backdrop"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
          className="fixed inset-0 z-40 flex items-start justify-center pt-24"
          style={{
            background: "rgba(0, 0, 0, 0.55)",
            backdropFilter: "blur(4px)",
          }}
          onClick={() => setOpen(false)}
        >
          <motion.div
            key="docs-card"
            initial={{ opacity: 0, scale: 0.98, y: -8 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.98, y: -8 }}
            transition={{ duration: 0.28, ease: [0.22, 1, 0.36, 1] }}
            className="relative rounded-2xl overflow-hidden shadow-2xl flex flex-col"
            style={{
              width: "min(680px, 92vw)",
              maxHeight: "min(560px, 70vh)",
              background: "rgb(12, 12, 16)",
              border: "1px solid rgba(255, 255, 255, 0.10)",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <div
              className="px-4 py-3 border-b flex items-center gap-2"
              style={{ borderColor: "rgba(255, 255, 255, 0.06)" }}
            >
              <span
                className="text-[10px] uppercase tracking-[0.24em] font-mono shrink-0"
                style={{ color: "rgba(236, 236, 241, 0.4)" }}
              >
                Documents
              </span>
              <input
                autoFocus
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="search docs…"
                className="flex-1 bg-transparent text-[13px] focus:outline-none placeholder:text-white/30"
                style={{ color: "rgba(236, 236, 241, 0.95)" }}
              />
              <span
                className="text-[9px] uppercase tracking-wider font-mono opacity-50"
                style={{ color: "rgba(236, 236, 241, 0.4)" }}
              >
                esc to close
              </span>
            </div>

            <div className="flex-1 min-h-0 overflow-y-auto">
              {loading && filtered.length === 0 && (
                <div
                  className="px-4 py-4 text-[11px] font-mono opacity-60"
                  style={{ color: "rgba(236, 236, 241, 0.6)" }}
                >
                  loading…
                </div>
              )}
              {error && (
                <div
                  className="px-4 py-2 text-[11px] font-mono"
                  style={{ color: "rgba(255, 130, 130, 0.85)" }}
                >
                  {error}
                </div>
              )}
              {!loading && filtered.length === 0 && !error && (
                <div
                  className="px-4 py-6 text-[12px] font-mono opacity-60 text-center"
                  style={{ color: "rgba(236, 236, 241, 0.6)" }}
                >
                  {q ? "nothing matched" : "no documents yet"}
                </div>
              )}
              {filtered.map((doc, i) => (
                <motion.button
                  key={doc.id}
                  initial={{ opacity: 0, y: 4 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{
                    duration: 0.24,
                    ease: [0.22, 1, 0.36, 1],
                    delay: Math.min(i * 0.012, 0.12),
                  }}
                  whileHover={{ backgroundColor: "rgba(255,255,255,0.03)" }}
                  onClick={() => handleSelect(doc)}
                  className="w-full text-left px-4 py-2.5 border-b flex items-center justify-between gap-3"
                  style={{ borderColor: "rgba(255,255,255,0.04)" }}
                >
                  <div className="min-w-0 flex-1">
                    <div
                      className="text-[13px] truncate"
                      style={{ color: "rgba(236, 236, 241, 0.92)" }}
                    >
                      {doc.displayName || doc.originalFilename}
                    </div>
                    <div
                      className="text-[10.5px] font-mono mt-0.5 truncate opacity-60"
                      style={{ color: "rgba(236, 236, 241, 0.7)" }}
                    >
                      {doc.kind} · {formatBytesShort(doc.sizeBytes)}
                    </div>
                  </div>
                </motion.button>
              ))}
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function formatBytesShort(n: number): string {
  if (n < 1024) return `${n}B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)}KB`;
  return `${(n / 1024 / 1024).toFixed(1)}MB`;
}
