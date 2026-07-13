import type { FieldDef } from "../packs";
import { Switch } from "../../ui/Switch";

/// Renders an input for a given field. The form treats the value as
/// `unknown` (matches the JSON shape that flows back into pack_table_
/// upsert) and emits typed values where possible — currency stays as
/// integer cents, numbers stay as numbers, bools stay as bools.
export function FieldInput({
  field,
  value,
  onChange,
  disabled = false,
}: {
  field: FieldDef;
  value: unknown;
  onChange: (next: unknown) => void;
  disabled?: boolean;
}) {
  const stringValue =
    value === null || value === undefined ? "" : String(value);

  const baseInput =
    "w-full rounded-md border border-ink-3 bg-ink-2/30 px-3 py-2 text-sm text-bone placeholder:text-bone-3/60 focus:border-pulse/70 focus:outline-none transition-colors disabled:opacity-50";

  switch (field.fieldType.kind) {
    case "longText":
      return (
        <textarea
          value={stringValue}
          rows={4}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value || null)}
          className={baseInput + " resize-y"}
        />
      );

    case "email":
      return (
        <input
          type="email"
          value={stringValue}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value || null)}
          className={baseInput}
        />
      );

    case "phone":
      return (
        <input
          type="tel"
          value={stringValue}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value || null)}
          className={baseInput}
        />
      );

    case "integer": {
      const n = typeof value === "number" ? value : Number(value);
      return (
        <input
          type="number"
          step="1"
          value={Number.isFinite(n) ? String(n) : ""}
          disabled={disabled}
          onChange={(e) => {
            const v = e.target.value;
            if (v === "") onChange(null);
            else {
              const parsed = parseInt(v, 10);
              onChange(Number.isFinite(parsed) ? parsed : null);
            }
          }}
          className={baseInput + " font-mono"}
        />
      );
    }

    case "number": {
      const n = typeof value === "number" ? value : Number(value);
      return (
        <input
          type="number"
          step="any"
          value={Number.isFinite(n) ? String(n) : ""}
          disabled={disabled}
          onChange={(e) => {
            const v = e.target.value;
            if (v === "") onChange(null);
            else {
              const parsed = parseFloat(v);
              onChange(Number.isFinite(parsed) ? parsed : null);
            }
          }}
          className={baseInput + " font-mono"}
        />
      );
    }

    case "currency": {
      // Stored as cents; display as dollars.
      const cents =
        typeof value === "number" ? value : value === null ? null : Number(value);
      const dollars =
        cents === null || !Number.isFinite(cents as number) ? "" : (cents! / 100).toFixed(2);
      return (
        <div className="flex items-center gap-1">
          <span className="text-bone-3 text-sm">$</span>
          <input
            type="number"
            step="0.01"
            value={dollars}
            disabled={disabled}
            onChange={(e) => {
              const v = e.target.value;
              if (v === "") onChange(null);
              else {
                const parsed = parseFloat(v);
                onChange(Number.isFinite(parsed) ? Math.round(parsed * 100) : null);
              }
            }}
            className={baseInput + " font-mono"}
          />
        </div>
      );
    }

    case "date":
      return (
        <input
          type="date"
          value={stringValue.slice(0, 10)}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value || null)}
          className={baseInput + " font-mono"}
        />
      );

    case "dateTime":
      return (
        <input
          type="datetime-local"
          value={stringValue.slice(0, 16)}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value || null)}
          className={baseInput + " font-mono"}
        />
      );

    case "bool":
      return (
        <div className="flex items-center gap-3">
          <Switch
            checked={!!value}
            disabled={disabled}
            onChange={(v) => onChange(v)}
            size="sm"
          />
          <span className="text-bone-2 text-sm">
            {value ? "Yes" : "No"}
          </span>
        </div>
      );

    case "enum":
      return (
        <select
          value={stringValue}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value || null)}
          className={baseInput}
        >
          <option value="" disabled={field.required}>
            {field.required ? "Pick one…" : "—"}
          </option>
          {field.fieldType.options.map((opt) => (
            <option key={opt} value={opt}>
              {opt}
            </option>
          ))}
        </select>
      );

    case "ref": {
      // Until the typeahead resolver lands, accept a numeric id.
      const n = typeof value === "number" ? value : Number(value);
      return (
        <input
          type="number"
          step="1"
          value={Number.isFinite(n) ? String(n) : ""}
          disabled={disabled}
          placeholder={`${field.fieldType.table} id`}
          onChange={(e) => {
            const v = e.target.value;
            if (v === "") onChange(null);
            else {
              const parsed = parseInt(v, 10);
              onChange(Number.isFinite(parsed) ? parsed : null);
            }
          }}
          className={baseInput + " font-mono"}
        />
      );
    }

    case "json":
      return (
        <textarea
          value={stringValue}
          rows={4}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value || null)}
          className={baseInput + " font-mono text-xs"}
        />
      );

    case "timestamp":
      // Read-only — DB-managed.
      return (
        <input
          type="text"
          value={stringValue}
          disabled
          className={baseInput + " font-mono opacity-60"}
        />
      );

    case "text":
    default:
      return (
        <input
          type="text"
          value={stringValue}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value || null)}
          className={baseInput}
        />
      );
  }
}
