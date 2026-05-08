import { useCallback, useEffect, useMemo, useState } from "react";
import {
  packTableList,
  type FieldDef,
  type PackSchema,
  type SortDir,
  type TableDef,
} from "../packs";
import { FieldCell } from "./FieldCell";

/// Auto-rendered list view for any pack table. Reads the table's
/// list_view config to pick columns + default sort, pulls rows via
/// pack_table_list, renders a sortable table with FieldCell per cell.
///
/// `onRowClick` and `onNew` are callbacks the parent (typically
/// `TableTab`) provides to navigate to detail / new-form views.
export function ListView({
  pack,
  table,
  onRowClick,
  onNew,
}: {
  pack: PackSchema;
  table: TableDef;
  onRowClick?: (id: number) => void;
  onNew?: () => void;
}) {
  const [rows, setRows] = useState<Record<string, unknown>[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [sortField, setSortField] = useState<string | null>(
    table.listView.defaultSort,
  );
  const [sortDir, setSortDir] = useState<SortDir>(table.listView.defaultSortDir);

  // Determine which columns to render. If the pack declared explicit
  // `columns`, use them. Otherwise fall back to every field marked
  // `defaultInList`.
  const columns = useMemo<string[]>(() => {
    if (table.listView.columns.length > 0) return table.listView.columns;
    return table.fields.filter((f) => f.defaultInList).map((f) => f.slug);
  }, [table]);

  const fieldByName = useMemo<Record<string, FieldDef>>(
    () => Object.fromEntries(table.fields.map((f) => [f.slug, f])),
    [table],
  );

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await packTableList({
        packSlug: pack.slug,
        tableSlug: table.slug,
        sort: sortField ?? undefined,
        sortDir,
      });
      setRows(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [pack.slug, table.slug, sortField, sortDir]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const onSortClick = (slug: string) => {
    if (sortField === slug) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortField(slug);
      setSortDir("asc");
    }
  };

  return (
    <div className="px-10 py-6 max-w-5xl mx-auto">
      <div className="flex items-baseline justify-between mb-4">
        <h2 className="text-bone text-lg font-light tracking-tight">
          {table.displayName}
        </h2>
        <div className="flex items-center gap-3">
          <span className="text-bone-3 text-[10px] font-mono">
            {pack.name} · {rows.length}
          </span>
          {onNew && (
            <button
              onClick={onNew}
              className="px-3 py-1 rounded-full bg-pulse/15 border border-pulse/40 text-bone-2 text-xs hover:bg-pulse/25 transition-colors"
            >
              + New {table.singularName}
            </button>
          )}
        </div>
      </div>

      {loading && rows.length === 0 ? (
        <p className="text-bone-3 text-xs">Loading…</p>
      ) : error ? (
        <p className="text-warn text-xs">{error}</p>
      ) : rows.length === 0 ? (
        <p className="text-bone-3 text-xs">
          No {table.displayName.toLowerCase()} yet.
        </p>
      ) : (
        <div className="rounded-xl border border-ink-3 overflow-hidden">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-ink-3 bg-ink-2/40">
                {columns.map((slug) => {
                  const field = fieldByName[slug];
                  if (!field) return null;
                  const isSort = sortField === slug;
                  return (
                    <th
                      key={slug}
                      onClick={() => onSortClick(slug)}
                      className={
                        "text-left px-4 py-2 text-[10px] tracking-[0.18em] uppercase cursor-pointer select-none transition-colors " +
                        (isSort
                          ? "text-bone-2"
                          : "text-bone-3 hover:text-bone-2")
                      }
                    >
                      {field.label}
                      {isSort && (
                        <span className="ml-1 text-pulse-2/70">
                          {sortDir === "asc" ? "↑" : "↓"}
                        </span>
                      )}
                    </th>
                  );
                })}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, i) => {
                const rowId =
                  typeof row.id === "number" ? row.id : Number(row.id);
                const clickable = onRowClick && Number.isFinite(rowId);
                return (
                  <tr
                    key={String(row.id ?? i)}
                    onClick={() => clickable && onRowClick!(rowId)}
                    className={
                      "border-b border-white/[0.03] transition-colors " +
                      (clickable
                        ? "cursor-pointer hover:bg-white/[0.04]"
                        : "hover:bg-white/[0.02]")
                    }
                  >
                    {columns.map((slug) => {
                      const field = fieldByName[slug];
                      if (!field) return null;
                      return (
                        <td key={slug} className="px-4 py-2.5">
                          <FieldCell field={field} value={row[slug]} />
                        </td>
                      );
                    })}
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
