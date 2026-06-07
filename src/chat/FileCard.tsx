import { useEffect, useState } from "react";
import {
  formatBytes,
  getDocument,
  previewDocument,
  type Document,
} from "../lib/documents";

interface Props {
  documentId: number;
}

/// Inline card for a generated or attached document. Shows icon, name,
/// size, kind, with quick-open via the OS default viewer. Used in chat
/// turns to surface generated PDFs / spreadsheets.
export function FileCard({ documentId }: Props) {
  const [doc, setDoc] = useState<Document | null>(null);
  const [opening, setOpening] = useState(false);

  useEffect(() => {
    getDocument(documentId)
      .then(setDoc)
      .catch(() => setDoc(null));
  }, [documentId]);

  if (!doc) {
    return (
      <div className="text-bone-3 text-[11px] font-mono px-3 py-2 my-1.5">
        loading doc#{documentId}…
      </div>
    );
  }

  const kindGlyph = (() => {
    if (doc.kind === "generated_pdf" || doc.mimeType === "application/pdf")
      return "📄";
    if (
      doc.kind === "generated_spreadsheet" ||
      doc.kind === "coach_hours_master" ||
      doc.mimeType.includes("spreadsheet")
    )
      return "📊";
    if (doc.mimeType.startsWith("image/")) return "🖼";
    return "📎";
  })();

  const handleOpen = async () => {
    setOpening(true);
    try {
      await previewDocument(documentId);
    } catch {
      /* ignore */
    } finally {
      setTimeout(() => setOpening(false), 300);
    }
  };

  return (
    <button
      onClick={handleOpen}
      disabled={opening}
      className="my-1.5 flex items-center gap-3 px-3 py-2 rounded-md text-left w-full max-w-[420px] transition-colors hover:bg-pulse/10"
      style={{
        background: "rgba(124, 92, 255, 0.06)",
        border: "1px solid rgba(124, 92, 255, 0.20)",
      }}
      title="Open in default viewer"
    >
      <span className="text-[20px] shrink-0">{kindGlyph}</span>
      <div className="flex-1 min-w-0">
        <div className="text-bone text-[13px] truncate">{doc.displayName}</div>
        <div className="text-bone-3 text-[10px] font-mono mt-0.5">
          {doc.kind} · {formatBytes(doc.sizeBytes)} · doc#{doc.id}
        </div>
      </div>
      <span className="text-bone-3 text-[11px] shrink-0">
        {opening ? "opening…" : "open"}
      </span>
    </button>
  );
}
