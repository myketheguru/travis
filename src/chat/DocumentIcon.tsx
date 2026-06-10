/**
 * v0.19.6 — Inline SVG icons for document kinds.
 *
 * Replaces the earlier emoji glyphs in DocumentsTab + FileCard with
 * consistent line-art icons. Single source of truth for "what does
 * an invoice / sample / PO / signing sheet / spreadsheet look like
 * as an icon in the chat or library." All icons are 1em-relative
 * so the same component renders cleanly inline next to a filename
 * at 13px and as a card glyph at 20px.
 *
 * Aesthetic: 1.4 stroke-width line icons in `currentColor`, so the
 * caller controls the tint (bone for default, pulse on hover, etc).
 * No dependency on lucide-react or any other icon library.
 */
import type { SVGProps } from "react";

export type DocumentIconKind =
  | "invoice"
  | "sample"
  | "contract"
  | "po"
  | "wo"
  | "signed_sheet"
  | "spreadsheet"
  | "image"
  | "pdf"
  | "docx"
  | "csv"
  | "text"
  | "code"
  | "archive"
  | "audio"
  | "video"
  | "presentation"
  | "file";

interface Props extends SVGProps<SVGSVGElement> {
  kind?: string;
  mimeType?: string;
  size?: number | string;
}

export function DocumentIcon({
  kind,
  mimeType,
  size = "1em",
  ...rest
}: Props) {
  const resolved = resolveKind(kind, mimeType);
  const common = {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.4,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
    ...rest,
  };

  switch (resolved) {
    case "invoice":
      // Page with $ on it + perforated edge at the bottom
      return (
        <svg {...common}>
          <path d="M5 3h11l3 3v15l-2-1.5L15 21l-3-1.5L9 21l-2-1.5L5 21z" />
          <line x1="9" y1="9" x2="14" y2="9" />
          <line x1="9" y1="13" x2="13" y2="13" />
          <line x1="9" y1="17" x2="11" y2="17" />
        </svg>
      );
    case "sample":
      // Sparkle on a doc — "use this as the template"
      return (
        <svg {...common}>
          <path d="M5 3h10l4 4v14H5z" />
          <path d="M14 13l1 2 2 1-2 1-1 2-1-2-2-1 2-1z" />
          <line x1="9" y1="9" x2="11" y2="9" />
          <line x1="9" y1="12" x2="10" y2="12" />
        </svg>
      );
    case "contract":
      // Doc with a seal/signature swirl bottom-right
      return (
        <svg {...common}>
          <path d="M5 3h11l4 4v14H5z" />
          <line x1="9" y1="9" x2="15" y2="9" />
          <line x1="9" y1="12" x2="15" y2="12" />
          <line x1="9" y1="15" x2="13" y2="15" />
          <circle cx="16" cy="17" r="2" />
        </svg>
      );
    case "po":
      // Doc with a numbered list (purchase order)
      return (
        <svg {...common}>
          <path d="M5 3h11l4 4v14H5z" />
          <line x1="11" y1="9" x2="16" y2="9" />
          <line x1="11" y1="12" x2="16" y2="12" />
          <line x1="11" y1="15" x2="16" y2="15" />
          <circle cx="8.5" cy="9" r="0.6" fill="currentColor" />
          <circle cx="8.5" cy="12" r="0.6" fill="currentColor" />
          <circle cx="8.5" cy="15" r="0.6" fill="currentColor" />
        </svg>
      );
    case "wo":
      // Doc with a wrench overlay (work order)
      return (
        <svg {...common}>
          <path d="M5 3h11l4 4v14H5z" />
          <line x1="9" y1="9" x2="15" y2="9" />
          <line x1="9" y1="12" x2="15" y2="12" />
          <path d="M14 16l3 3M15.5 17.5l-1 1 1.5 1.5 1-1z" />
        </svg>
      );
    case "signed_sheet":
      // Clipboard with horizontal lines = sign-in sheet
      return (
        <svg {...common}>
          <rect x="6" y="4" width="12" height="17" rx="1.5" />
          <rect x="9" y="2.5" width="6" height="3" rx="0.8" />
          <line x1="8.5" y1="10" x2="15.5" y2="10" />
          <line x1="8.5" y1="13" x2="15.5" y2="13" />
          <line x1="8.5" y1="16" x2="13.5" y2="16" />
        </svg>
      );
    case "spreadsheet":
      // Grid icon
      return (
        <svg {...common}>
          <path d="M5 3h14v18H5z" />
          <line x1="5" y1="8" x2="19" y2="8" />
          <line x1="5" y1="13" x2="19" y2="13" />
          <line x1="5" y1="18" x2="19" y2="18" />
          <line x1="10" y1="3" x2="10" y2="21" />
          <line x1="14.5" y1="3" x2="14.5" y2="21" />
        </svg>
      );
    case "image":
      // Mountain / sun
      return (
        <svg {...common}>
          <rect x="4" y="5" width="16" height="14" rx="1.5" />
          <circle cx="9" cy="10" r="1.4" />
          <path d="M4 17l5-5 4 4 3-3 4 4" />
        </svg>
      );
    case "pdf":
      return (
        <svg {...common}>
          <path d="M5 3h11l4 4v14H5z" />
          <path d="M16 3v4h4" />
          <text x="12" y="16" textAnchor="middle" fontSize="5" fontWeight="600" stroke="none" fill="currentColor">PDF</text>
        </svg>
      );
    case "docx":
      return (
        <svg {...common}>
          <path d="M5 3h11l4 4v14H5z" />
          <path d="M16 3v4h4" />
          <text x="12" y="16" textAnchor="middle" fontSize="4.5" fontWeight="600" stroke="none" fill="currentColor">DOC</text>
        </svg>
      );
    case "csv":
      return (
        <svg {...common}>
          <path d="M5 3h11l4 4v14H5z" />
          <path d="M16 3v4h4" />
          <text x="12" y="16" textAnchor="middle" fontSize="4.5" fontWeight="600" stroke="none" fill="currentColor">CSV</text>
        </svg>
      );
    case "text":
      return (
        <svg {...common}>
          <path d="M5 3h11l4 4v14H5z" />
          <line x1="9" y1="9" x2="15" y2="9" />
          <line x1="9" y1="12" x2="15" y2="12" />
          <line x1="9" y1="15" x2="15" y2="15" />
          <line x1="9" y1="18" x2="13" y2="18" />
        </svg>
      );
    case "code":
      return (
        <svg {...common}>
          <path d="M5 3h11l4 4v14H5z" />
          <polyline points="10 12 8 14 10 16" />
          <polyline points="14 12 16 14 14 16" />
        </svg>
      );
    case "archive":
      return (
        <svg {...common}>
          <rect x="4" y="6" width="16" height="14" rx="1.5" />
          <line x1="4" y1="10" x2="20" y2="10" />
          <line x1="12" y1="6" x2="12" y2="14" />
          <rect x="11" y="13" width="2" height="3" fill="currentColor" stroke="none" />
        </svg>
      );
    case "audio":
      return (
        <svg {...common}>
          <path d="M5 3h11l4 4v14H5z" />
          <circle cx="10" cy="16" r="1.8" />
          <path d="M11.8 16V9l4 1.5v6" />
          <circle cx="14" cy="17" r="1.6" />
        </svg>
      );
    case "video":
      return (
        <svg {...common}>
          <rect x="3" y="6" width="14" height="12" rx="1" />
          <path d="M17 10l4-2v8l-4-2z" />
        </svg>
      );
    case "presentation":
      return (
        <svg {...common}>
          <rect x="3" y="4" width="18" height="12" rx="1" />
          <line x1="9" y1="20" x2="15" y2="20" />
          <line x1="12" y1="16" x2="12" y2="20" />
          <polyline points="7 12 10 9 13 12 17 8" />
        </svg>
      );
    case "file":
    default:
      // Generic paperclip-on-page
      return (
        <svg {...common}>
          <path d="M5 3h11l4 4v14H5z" />
          <path d="M16 3v4h4" />
          <path d="M14 11l-3.5 3.5a1.8 1.8 0 102.5 2.5L17 13" />
        </svg>
      );
  }
}

function resolveKind(kind?: string, mimeType?: string): DocumentIconKind {
  const k = (kind ?? "").toLowerCase();
  const m = (mimeType ?? "").toLowerCase();
  if (k.includes("invoice")) return "invoice";
  if (k.includes("sample")) return "sample";
  if (k.includes("contract")) return "contract";
  if (k === "po" || k === "purchase_order") return "po";
  if (k === "wo" || k === "work_order") return "wo";
  if (k.includes("signed_sheet") || k.includes("signing_sheet") || k.includes("sign_in") || k.includes("master")) {
    return "signed_sheet";
  }
  if (m.includes("presentation") || m.includes("powerpoint") || k.includes("presentation") || k.includes("slides")) {
    return "presentation";
  }
  if (k === "generated_csv" || m.includes("csv") || k === "csv") return "csv";
  if (k.includes("spreadsheet") || m.includes("spreadsheet") || m.includes("excel")) return "spreadsheet";
  if (m.includes("zip") || m.includes("compressed") || k.includes("archive") || k.includes("zip")) return "archive";
  if (m.startsWith("audio/")) return "audio";
  if (m.startsWith("video/")) return "video";
  if (m.startsWith("image/")) return "image";
  if (m === "application/pdf" || k.includes("pdf")) return "pdf";
  if (m.includes("word") || k === "generated_doc" || k === "docx" || k === "doc") return "docx";
  if (m.startsWith("text/") || k.includes("text") || k.includes("note") || k === "txt" || k === "md") return "text";
  if (k.includes("code") || k.includes("script") || k === "py" || k === "js" || k === "ts" || k === "json") return "code";
  return "file";
}
