import { useEffect, useState } from "react";
import {
  packTableDelete,
  packTableGet,
  type PackSchema,
  type TableDef,
} from "../packs";
import { FieldCell } from "./FieldCell";

/// Auto-rendered read-only detail view. Loads a single row via
/// pack_table_get and renders every field in a labeled list. Edit and
/// Delete buttons emit callbacks the caller wires up to navigation.
export function DetailView({
  pack,
  table,
  id,
  onClose,
  onEdit,
  onDeleted,
}: {
  pack: PackSchema;
  table: TableDef;
  id: number;
  onClose: () => void;
  onEdit: () => void;
  onDeleted: () => void;
}) {
  const [row, setRow] = useState<Record<string, unknown> | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);

  useEffect(() => {
    setLoading(true);
    setError(null);
    packTableGet({ packSlug: pack.slug, tableSlug: table.slug, id })
      .then(setRow)
      .catch((e) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setLoading(false));
  }, [pack.slug, table.slug, id]);

  const deleteRow = async () => {
    setError(null);
    try {
      await packTableDelete({ packSlug: pack.slug, tableSlug: table.slug, id });
      onDeleted();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setConfirmDelete(false);
    }
  };

  const titleValue = row?.[table.displayField];

  return (
    <div className="px-10 py-6 max-w-2xl mx-auto">
      <div className="flex items-center justify-between mb-5">
        <button
          onClick={onClose}
          className="text-bone-3 hover:text-bone-2 text-xs flex items-center gap-1.5"
        >
          <span aria-hidden>←</span>
          <span>Back to {table.displayName}</span>
        </button>
        {row && (
          <div className="flex items-center gap-2">
            <button
              onClick={onEdit}
              className="px-3 py-1.5 rounded-full border border-ink-3 hover:border-pulse/40 text-bone-2 text-xs transition-colors"
            >
              Edit
            </button>
            {confirmDelete ? (
              <>
                <button
                  onClick={deleteRow}
                  className="px-3 py-1.5 rounded-full bg-warn/20 border border-warn/40 text-warn text-xs"
                >
                  Confirm delete
                </button>
                <button
                  onClick={() => setConfirmDelete(false)}
                  className="text-bone-3 hover:text-bone-2 text-xs"
                >
                  Cancel
                </button>
              </>
            ) : (
              <button
                onClick={() => setConfirmDelete(true)}
                className="px-3 py-1.5 rounded-full border border-ink-3 hover:border-warn/40 text-bone-3 hover:text-warn text-xs transition-colors"
              >
                Delete
              </button>
            )}
          </div>
        )}
      </div>

      {loading ? (
        <p className="text-bone-3 text-xs">Loading…</p>
      ) : error ? (
        <p className="text-warn text-xs">{error}</p>
      ) : !row ? (
        <p className="text-bone-3 text-xs">Not found.</p>
      ) : (
        <>
          <h2 className="text-bone text-2xl font-light tracking-tight mb-5">
            {titleValue !== null && titleValue !== undefined && titleValue !== ""
              ? String(titleValue)
              : `${table.singularName} #${id}`}
          </h2>

          <div className="rounded-xl border border-ink-3 bg-ink-2/30 divide-y divide-white/[0.04]">
            {table.fields.map((f) => (
              <div key={f.slug} className="grid grid-cols-[140px_1fr] gap-4 px-4 py-3">
                <span className="text-bone-3 text-[10px] tracking-[0.18em] uppercase pt-0.5">
                  {f.label}
                </span>
                <div className="text-sm">
                  <FieldCell field={f} value={row[f.slug]} />
                  {f.help && (
                    <p className="text-bone-3 text-[11px] mt-0.5 leading-relaxed opacity-70">
                      {f.help}
                    </p>
                  )}
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
