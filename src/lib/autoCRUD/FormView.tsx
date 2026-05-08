import { useEffect, useState } from "react";
import {
  packTableGet,
  packTableUpsert,
  type FieldDef,
  type FieldType,
  type PackSchema,
  type TableDef,
} from "../packs";
import { FieldInput } from "./FieldInput";

/// Auto-rendered create / edit form. Reads the table's fields, renders one
/// input per editable field via FieldInput, and persists via
/// pack_table_upsert. Pass `id` to edit an existing row; omit it to
/// create a new one.
export function FormView({
  pack,
  table,
  id,
  onCancel,
  onSaved,
}: {
  pack: PackSchema;
  table: TableDef;
  id?: number;
  onCancel: () => void;
  onSaved: (row: Record<string, unknown>) => void;
}) {
  const [draft, setDraft] = useState<Record<string, unknown>>({});
  const [loading, setLoading] = useState(id !== undefined);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (id === undefined) {
      // Initialise blank draft with default null values.
      const blank: Record<string, unknown> = {};
      for (const f of table.fields) {
        blank[f.slug] = null;
      }
      setDraft(blank);
      setLoading(false);
      return;
    }

    setLoading(true);
    setError(null);
    packTableGet({ packSlug: pack.slug, tableSlug: table.slug, id })
      .then((row) => setDraft(row))
      .catch((e) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setLoading(false));
  }, [pack.slug, table.slug, id, table.fields]);

  const editableFields = table.fields.filter((f) => !isReadOnly(f));

  const validate = (): string | null => {
    for (const f of table.fields) {
      if (!f.required) continue;
      if (isReadOnly(f)) continue;
      const v = draft[f.slug];
      if (v === null || v === undefined || v === "") {
        return `'${f.label}' is required`;
      }
    }
    return null;
  };

  const save = async () => {
    const err = validate();
    if (err) {
      setError(err);
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const payload: Record<string, unknown> = { ...draft };
      // Only include id when editing (so the backend recognises it as
      // an update vs an insert).
      if (id === undefined) {
        delete payload.id;
      }
      const saved = await packTableUpsert({
        packSlug: pack.slug,
        tableSlug: table.slug,
        payload,
      });
      onSaved(saved);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="px-10 py-6 max-w-2xl mx-auto">
      <div className="flex items-baseline justify-between mb-5">
        <h2 className="text-bone text-lg font-light tracking-tight">
          {id === undefined ? `New ${table.singularName}` : `Edit ${table.singularName}`}
        </h2>
        <button
          onClick={onCancel}
          className="text-bone-3 hover:text-bone-2 text-xs"
        >
          Cancel
        </button>
      </div>

      {loading ? (
        <p className="text-bone-3 text-xs">Loading…</p>
      ) : (
        <div className="flex flex-col gap-5">
          {editableFields.map((f) => (
            <label key={f.slug} className="flex flex-col gap-1.5">
              <span className="text-bone-3 text-[10px] tracking-[0.18em] uppercase">
                {f.label}
                {f.required && <span className="text-warn ml-1">*</span>}
              </span>
              <FieldInput
                field={f}
                value={draft[f.slug]}
                onChange={(v) =>
                  setDraft((d) => ({ ...d, [f.slug]: v }))
                }
                disabled={saving}
              />
              {f.help && (
                <span className="text-bone-3 text-[11px] leading-relaxed">
                  {f.help}
                </span>
              )}
            </label>
          ))}

          {error && <p className="text-warn text-xs">{error}</p>}

          <div className="flex items-center gap-3 pt-2">
            <button
              onClick={save}
              disabled={saving}
              className="px-4 py-2 rounded-full bg-bone/95 text-ink text-sm font-medium hover:bg-bone disabled:opacity-50 transition-colors"
            >
              {saving ? "Saving…" : "Save"}
            </button>
            <button
              onClick={onCancel}
              disabled={saving}
              className="px-4 py-2 rounded-full text-bone-3 hover:text-bone-2 text-sm transition-colors"
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function isReadOnly(field: FieldDef): boolean {
  return field.slug === "id" || isTimestamp(field.fieldType);
}

function isTimestamp(t: FieldType): boolean {
  return t.kind === "timestamp";
}
