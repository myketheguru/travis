/**
 * DocumentViewerOverlay — v0.28.73.
 *
 * v2 workspace's DocumentViewer surface. The old Manage view was the
 * only thing that rendered <DocumentViewer/> when `viewerDocumentId`
 * flipped non-null; in v2 the store field was orphaned so clicking a
 * FileCard did nothing. This overlay watches viewerDocumentId and
 * renders DocumentViewer inside a modal-style backdrop.
 *
 * Esc closes. Backdrop click closes.
 */
import { useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useAppStore } from "../../stores/app";
import { DocumentViewer } from "../../chat/DocumentViewer";

export function DocumentViewerOverlay() {
  const viewerDocumentId = useAppStore((s) => s.viewerDocumentId);
  const setViewerDocumentId = useAppStore((s) => s.setViewerDocumentId);

  useEffect(() => {
    if (viewerDocumentId === null) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        setViewerDocumentId(null);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [viewerDocumentId, setViewerDocumentId]);

  return (
    <AnimatePresence>
      {viewerDocumentId !== null && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
          className="fixed inset-0 z-50 flex items-center justify-center"
          style={{
            background: "rgba(7, 8, 11, 0.72)",
            backdropFilter: "blur(6px)",
            WebkitBackdropFilter: "blur(6px)",
          }}
          onClick={(e) => {
            if (e.target === e.currentTarget) setViewerDocumentId(null);
          }}
        >
          <motion.div
            initial={{ scale: 0.94, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.96, opacity: 0 }}
            transition={{ duration: 0.28, ease: [0.22, 1, 0.36, 1] }}
            className="relative"
            style={{
              width: "min(1000px, 92vw)",
              height: "min(80vh, 780px)",
              borderRadius: 16,
              overflow: "hidden",
              border: "1px solid rgba(255, 255, 255, 0.08)",
              background: "#0e1015",
              boxShadow: "0 40px 80px -20px rgba(0, 0, 0, 0.75)",
            }}
          >
            <button
              onClick={() => setViewerDocumentId(null)}
              aria-label="Close document"
              style={{
                position: "absolute",
                top: 12,
                right: 12,
                zIndex: 10,
                width: 28,
                height: 28,
                borderRadius: "50%",
                background: "rgba(255, 255, 255, 0.06)",
                border: "1px solid rgba(255, 255, 255, 0.10)",
                color: "rgba(236, 236, 241, 0.75)",
                fontSize: 14,
                cursor: "pointer",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              ×
            </button>
            <div className="absolute inset-0">
              <DocumentViewer documentId={viewerDocumentId} />
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
