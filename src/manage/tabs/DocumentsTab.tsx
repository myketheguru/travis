/**
 * v0.19.6 — Documents tab.
 *
 * First-class core surface for every file Travis has seen: things
 * the user dropped (POs, WOs, signing sheets, samples, contracts),
 * things Travis generated (invoices, sign-in sheets, derived
 * artifacts), things imported in bulk. The library; the place to
 * find a file by category, name, date, or originating chat.
 *
 * Categorization comes from the document `kind` column — populated
 * either by the user (drag-and-drop + manual set), by the LLM
 * (`documentClassifications` extraction), or by Travis at generation
 * time (run_python outputs land with a generated_* kind).
 *
 * "Attach to current chat" is the bridge back to a conversation —
 * pulls the doc into the active AskTab attachment list so Travis
 * can reference it without re-uploading.
 */
import { useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import {
  formatBytes,
  listDocuments,
  previewDocument,
  revealDocumentInFolder,
  setDocumentKind,
  type Document,
} from "../../lib/documents";
import { useAppStore } from "../../stores/app";
import { DocumentIcon } from "../../chat/DocumentIcon";

const KIND_CATEGORIES: { id: string; label: string; matchers: string[] }[] = [
  { id: "all",          label: "All",                matchers: [] },
  { id: "samples",      label: "Samples",            matchers: ["sample", "sample_invoice", "sample_pdf"] },
  { id: "invoices",     label: "Invoices",           matchers: ["invoice", "generated_invoice"] },
  { id: "purchase",     label: "POs / WOs",          matchers: ["po", "wo", "purchase_order", "work_order"] },
  { id: "signing",      label: "Sign-in sheets",     matchers: ["signed_sheet", "signing_sheet", "coach_hours_master", "generated_sign_in_sheet"] },
  { id: "contracts",    label: "Contracts",          matchers: ["contract"] },
  { id: "spreadsheets", label: "Spreadsheets",       matchers: ["spreadsheet", "generated_spreadsheet"] },
  { id: "pdfs",         label: "PDFs (other)",       matchers: ["pdf", "generated_pdf"] },
  { id: "uncategorized",label: "Uncategorized",      matchers: ["file"] },
];

const KIND_OPTIONS = [
  "file",
  "po", "wo", "signed_sheet", "contract",
  "invoice", "sample_invoice", "sample_pdf",
  "generated_invoice", "generated_sign_in_sheet",
  "generated_pdf", "generated_spreadsheet", "generated_csv", "generated_doc",
];

const SOURCE_LABEL: Record<string, string> = {
  user_dropped: "Uploaded",
  generated_by_travis: "Generated",
  imported: "Imported",
};

export default function DocumentsTab() {
  const setActiveConversationId = useAppStore((s) => s.setActiveConversationId);

  const [docs, setDocs] = useState<Document[]>([]);
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("all");
  const [loading, setLoading] = useState(true);
  const [refreshTick, setRefreshTick] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listDocuments({ limit: 500 })
      .then((d) => {
        if (!cancelled) setDocs(d);
      })
      .catch(() => {
        if (!cancelled) setDocs([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [refreshTick]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    const cat = KIND_CATEGORIES.find((k) => k.id === category);
    return docs.filter((d) => {
      if (cat && cat.matchers.length > 0) {
        const k = (d.kind || "").toLowerCase();
        if (!cat.matchers.some((m) => k === m || k.includes(m))) {
          return false;
        }
      }
      if (q && !d.displayName.toLowerCase().includes(q) && !d.originalFilename.toLowerCase().includes(q)) {
        return false;
      }
      return true;
    });
  }, [docs, query, category]);

  const categoryCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const c of KIND_CATEGORIES) {
      if (c.matchers.length === 0) {
        counts[c.id] = docs.length;
      } else {
        counts[c.id] = docs.filter((d) => {
          const k = (d.kind || "").toLowerCase();
          return c.matchers.some((m) => k === m || k.includes(m));
        }).length;
      }
    }
    return counts;
  }, [docs]);

  const handleOpen = async (doc: Document) => {
    try {
      await previewDocument(doc.id);
    } catch {
      /* ignore */
    }
  };

  const handleReveal = async (doc: Document) => {
    try {
      await revealDocumentInFolder(doc.id);
    } catch {
      /* ignore */
    }
  };

  const handleSetKind = async (doc: Document, newKind: string) => {
    try {
      await setDocumentKind(doc.id, newKind);
      setRefreshTick((n) => n + 1);
    } catch {
      /* ignore */
    }
  };

  const handleAttachToChat = (doc: Document) => {
    // Hand off via a window-level custom event AskTab listens for.
    // Bridges without introducing a hard dep between tabs.
    const evt = new CustomEvent("travis:attach-document-from-library", {
      detail: { documentId: doc.id, displayName: doc.displayName },
    });
    window.dispatchEvent(evt);
  };

  const handleJumpToConversation = (doc: Document) => {
    if (doc.conversationId == null) return;
    setActiveConversationId(doc.conversationId);
  };

  return (
    <div className="h-full flex flex-col p-6 gap-4">
      <header className="flex items-baseline justify-between gap-4">
        <div>
          <h1 className="text-bone text-lg font-light tracking-tight">Documents</h1>
          <p className="text-bone-3 text-[11px] mt-0.5">
            Every file Travis has seen — yours and his. Filter, search, open in
            the chat, or jump to where it came from.
          </p>
        </div>
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search by name…"
          className="bg-white/[0.03] rounded-md px-3 py-1.5 text-[12px] text-bone outline-none focus:bg-white/[0.05] placeholder:text-bone-3/60 min-w-[220px]"
        />
      </header>

      <nav className="flex flex-wrap gap-1.5">
        {KIND_CATEGORIES.map((c) => {
          const isActive = category === c.id;
          const count = categoryCounts[c.id] ?? 0;
          return (
            <button
              key={c.id}
              onClick={() => setCategory(c.id)}
              className={
                "px-2.5 py-1 rounded text-[11px] transition-colors flex items-center gap-1.5 " +
                (isActive
                  ? "bg-pulse/[0.12] text-bone"
                  : "text-bone-2 hover:bg-white/[0.04]")
              }
            >
              {c.label}
              <span className="text-bone-3/60 tabular-nums">{count}</span>
            </button>
          );
        })}
      </nav>

      <div className="flex-1 min-h-0 overflow-y-auto">
        {loading && docs.length === 0 && (
          <div className="text-bone-3 text-xs text-center py-12">Loading…</div>
        )}
        {!loading && filtered.length === 0 && (
          <div className="text-bone-3 text-xs text-center py-12">
            {query
              ? `No documents matching "${query}".`
              : category === "all"
              ? "No documents yet. Drop a file in the chat or generate one with Travis to start populating this library."
              : "Nothing in this category yet."}
          </div>
        )}
        <ul className="space-y-1.5">
          {filtered.map((doc) => (
            <motion.li
              key={doc.id}
              layout
              initial={{ opacity: 0, y: 2 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.16 }}
              className="rounded-md border border-white/[0.05] hover:border-white/[0.10] bg-white/[0.015] p-3 flex items-start gap-3"
            >
              <span className="shrink-0 text-bone-2 mt-0.5">
                <DocumentIcon kind={doc.kind} mimeType={doc.mimeType} size={22} />
              </span>
              <div className="flex-1 min-w-0">
                <div className="flex items-baseline justify-between gap-2">
                  <button
                    onClick={() => handleOpen(doc)}
                    className="text-bone text-[13px] truncate hover:text-pulse text-left"
                    title="Open in default viewer"
                  >
                    {doc.displayName}
                  </button>
                  <span className="text-bone-3 text-[10px] tabular-nums shrink-0">
                    {formatBytes(doc.sizeBytes)}
                  </span>
                </div>
                <div className="text-bone-3 text-[10px] font-mono mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-0.5">
                  <select
                    value={doc.kind}
                    onChange={(e) => handleSetKind(doc, e.target.value)}
                    onClick={(e) => e.stopPropagation()}
                    className="bg-white/[0.04] hover:bg-white/[0.07] rounded px-1.5 py-0.5 text-bone-2 cursor-pointer"
                    title="Set the document kind"
                  >
                    {!KIND_OPTIONS.includes(doc.kind) && (
                      <option value={doc.kind}>{doc.kind}</option>
                    )}
                    {KIND_OPTIONS.map((k) => (
                      <option key={k} value={k}>
                        {k}
                      </option>
                    ))}
                  </select>
                  <span>·</span>
                  <span>{SOURCE_LABEL[doc.source] ?? doc.source}</span>
                  <span>·</span>
                  <span>{formatDate(doc.createdAt)}</span>
                  {doc.conversationId != null && (
                    <>
                      <span>·</span>
                      <button
                        onClick={() => handleJumpToConversation(doc)}
                        className="hover:text-pulse text-left"
                        title="Jump to the conversation this doc came from"
                      >
                        from chat #{doc.conversationId}
                      </button>
                    </>
                  )}
                </div>
              </div>
              <div className="flex flex-col gap-1 shrink-0">
                <button
                  onClick={() => handleAttachToChat(doc)}
                  className="text-[10px] text-bone-2 hover:text-bone bg-pulse/[0.10] hover:bg-pulse/[0.20] rounded px-2 py-0.5 transition-colors"
                  title="Attach to the active chat"
                >
                  attach to chat
                </button>
                <button
                  onClick={() => handleReveal(doc)}
                  className="text-[10px] text-bone-3 hover:text-bone-2 hover:bg-white/[0.05] rounded px-2 py-0.5 transition-colors"
                  title="Show file in your file manager"
                >
                  reveal
                </button>
              </div>
            </motion.li>
          ))}
        </ul>
      </div>
    </div>
  );
}

function formatDate(iso: string): string {
  const tsRaw = iso.includes("T") ? iso : iso.replace(" ", "T") + "Z";
  const t = Date.parse(tsRaw);
  if (!Number.isFinite(t)) return iso;
  const d = new Date(t);
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}
