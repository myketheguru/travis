/**
 * Inline document reference card. Renders a preview strip with the
 * doc title, kind icon, snippet if available, and an "Open" button
 * that hands off to the existing DocumentViewer.
 *
 * God's-Eye principle (INTERFACE.md): "show me the March report"
 * SHOULD produce this card + open the doc, not a text summary.
 */

import { useEffect, useState } from "react";
import { getDocument, type Document } from "../../lib/documents";
import { DocumentIcon } from "../DocumentIcon";
import { useAppStore } from "../../stores/app";

interface Props {
  documentId: number;
  snippet?: string;
  narration?: string;
}

export function DocRefCard({ documentId, snippet, narration }: Props) {
  const [doc, setDoc] = useState<Document | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const setViewerDocumentId = useAppStore((s) => s.setViewerDocumentId);

  useEffect(() => {
    let cancelled = false;
    getDocument(documentId)
      .then((d) => {
        if (!cancelled) setDoc(d);
      })
      .catch((e) => {
        if (!cancelled) setErr(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [documentId]);

  if (err) {
    return (
      <div
        className="rounded-lg border px-3 py-2 text-xs"
        style={{
          borderColor: "rgba(232, 154, 154, 0.35)",
          background: "rgba(232, 154, 154, 0.06)",
          color: "rgba(236, 236, 241, 0.85)",
        }}
      >
        Couldn't load doc {documentId}: {err}
      </div>
    );
  }

  return (
    <button
      onClick={() => setViewerDocumentId(documentId)}
      disabled={!doc}
      className="w-full text-left rounded-2xl transition-colors disabled:opacity-50"
      style={{
        border: "1px solid rgba(110, 196, 232, 0.35)",
        background:
          "linear-gradient(180deg, rgba(110, 196, 232, 0.05), rgba(124, 92, 255, 0.03))",
        boxShadow: "0 4px 20px -12px rgba(0, 0, 0, 0.4)",
      }}
    >
      <div className="flex items-center gap-3 p-3">
        <div
          className="shrink-0 h-10 w-10 rounded-lg flex items-center justify-center"
          style={{ background: "rgba(255, 255, 255, 0.05)" }}
        >
          <DocumentIcon
            kind={doc?.kind ?? "document"}
            mimeType={doc?.mimeType}
            size={20}
          />
        </div>
        <div className="flex-1 min-w-0">
          <div
            className="text-[10px] tracking-[0.18em] uppercase font-mono mb-0.5"
            style={{ color: "rgba(236, 236, 241, 0.5)" }}
          >
            // doc
          </div>
          <div
            className="text-[14px] font-medium truncate"
            style={{ color: "rgb(236, 236, 241)" }}
          >
            {doc?.originalFilename ?? "Loading…"}
          </div>
          {(snippet || narration) && (
            <div
              className="text-[11.5px] mt-1 leading-relaxed line-clamp-2"
              style={{ color: "rgba(236, 236, 241, 0.65)" }}
            >
              {snippet ?? narration}
            </div>
          )}
        </div>
        <span
          className="shrink-0 text-[11px] uppercase tracking-wider font-mono"
          style={{ color: "rgba(110, 196, 232, 0.85)" }}
        >
          open →
        </span>
      </div>
    </button>
  );
}
