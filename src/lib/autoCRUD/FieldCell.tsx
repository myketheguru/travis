import type { FieldDef } from "../packs";

/// Renders a single value for a list-view cell or detail-view field.
/// The presentation rules come from `field.fieldType` — currency is
/// "$X.YY", dates are sliced to YYYY-MM-DD, refs show "#id" until the
/// reference resolver lands.
export function FieldCell({
  field,
  value,
}: {
  field: FieldDef;
  value: unknown;
}) {
  if (value === null || value === undefined || value === "") {
    return <span className="text-bone-3 opacity-40">—</span>;
  }

  switch (field.fieldType.kind) {
    case "currency": {
      const cents = typeof value === "number" ? value : Number(value);
      if (!Number.isFinite(cents)) {
        return <span className="text-bone-3">{String(value)}</span>;
      }
      const dollars = cents / 100;
      return (
        <span className="text-bone-2 font-mono">${dollars.toFixed(2)}</span>
      );
    }

    case "number":
    case "integer":
      return (
        <span className="text-bone-2 font-mono">{String(value)}</span>
      );

    case "date":
      return (
        <span className="text-bone-2 font-mono">
          {String(value).slice(0, 10)}
        </span>
      );

    case "dateTime":
    case "timestamp":
      // Render YYYY-MM-DD HH:MM for compactness.
      return (
        <span className="text-bone-2 font-mono">
          {String(value).slice(0, 16).replace("T", " ")}
        </span>
      );

    case "ref":
      // Reference resolution (showing the ref'd row's display field) is a
      // follow-up. For now show the integer id with a hint about the table.
      return (
        <span className="text-pulse-2/80 font-mono">
          {field.fieldType.table}#{String(value)}
        </span>
      );

    case "bool":
      return (
        <span className="text-bone-2">{value ? "Yes" : "No"}</span>
      );

    case "enum":
      return (
        <span className="rounded-full border border-ink-3 bg-ink-2/40 px-2 py-0.5 text-[10px] tracking-wider text-bone-2 uppercase">
          {String(value)}
        </span>
      );

    case "longText":
      // Truncate long text in list cells; full text in detail.
      return (
        <span className="text-bone-2 line-clamp-2">{String(value)}</span>
      );

    case "json":
      return (
        <span className="text-bone-3 font-mono text-[10px] line-clamp-1">
          {String(value)}
        </span>
      );

    case "text":
    case "email":
    case "phone":
    default:
      return <span className="text-bone-2">{String(value)}</span>;
  }
}
