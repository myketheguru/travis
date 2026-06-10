import { useCallback, useState } from "react";
import type { PackSchema, TableDef } from "../packs";
import { ListView } from "./ListView";
import { DetailView } from "./DetailView";
import { FormView } from "./FormView";
import { getOverride } from "../packRegistry";

type Mode =
  | { kind: "list"; nonce: number }
  | { kind: "new"; nonce: number }
  | { kind: "detail"; id: number; nonce: number }
  | { kind: "edit"; id: number; nonce: number };

/// Single component that owns the list/detail/form navigation for one
/// pack table. The Manage tab renders this; it switches between
/// sub-views based on user clicks. The `nonce` field forces the
/// downstream component to re-mount after CRUD operations so its data
/// is fresh.
export function TableTab({
  pack,
  table,
}: {
  pack: PackSchema;
  table: TableDef;
}) {
  const [mode, setMode] = useState<Mode>({ kind: "list", nonce: 0 });

  const goList = useCallback(
    () => setMode({ kind: "list", nonce: Date.now() }),
    [],
  );

  switch (mode.kind) {
    case "list":
      return (
        <div key={mode.nonce}>
          <ListView
            pack={pack}
            table={table}
            onRowClick={(id) => setMode({ kind: "detail", id, nonce: 0 })}
            onNew={() => setMode({ kind: "new", nonce: 0 })}
          />
        </div>
      );

    case "new":
      return (
        <FormView
          pack={pack}
          table={table}
          onCancel={goList}
          onSaved={(row) => {
            const id = typeof row.id === "number" ? row.id : Number(row.id);
            if (Number.isFinite(id)) {
              setMode({ kind: "detail", id, nonce: 0 });
            } else {
              goList();
            }
          }}
        />
      );

    case "detail": {
      // v0.20.0 — packs can ship a custom detail-view component
      // (e.g. SchoolDetail) for relationship-aware drill-downs that
      // need to load + join across multiple pack tables. The
      // override receives id + onClose; everything else is the
      // pack's responsibility.
      const Override = getOverride(pack.slug, table.slug, "detail");
      if (Override) {
        return <Override id={mode.id} onClose={goList} />;
      }
      return (
        <DetailView
          pack={pack}
          table={table}
          id={mode.id}
          onClose={goList}
          onEdit={() => setMode({ kind: "edit", id: mode.id, nonce: 0 })}
          onDeleted={goList}
        />
      );
    }

    case "edit":
      return (
        <FormView
          pack={pack}
          table={table}
          id={mode.id}
          onCancel={() => setMode({ kind: "detail", id: mode.id, nonce: 0 })}
          onSaved={() => setMode({ kind: "detail", id: mode.id, nonce: Date.now() })}
        />
      );
  }
}
