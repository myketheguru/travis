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
      // Page with PDF micro-label
      return (
        <svg {...common}>
          <path d="M5 3h11l4 4v14H5z" />
          <path d="M16 3v4h4" />
          <text
            x="12"
            y="16"
            textAnchor="middle"
            fontSize="5"
            fontWeight="600"
            stroke="none"
            fill="currentColor"
          >
            PDF
          </text>
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
  if (k.includes("spreadsheet") || k === "generated_csv" || m.includes("spreadsheet") || m.includes("excel") || m.includes("csv")) {
    return "spreadsheet";
  }
  if (m.startsWith("image/")) return "image";
  if (m === "application/pdf" || k.includes("pdf")) return "pdf";
  return "file";
}
