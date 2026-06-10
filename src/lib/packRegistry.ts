import type { ComponentType } from "react";

// Per-pack UI component registries. Each pack ships its own
// `src/packs/<slug>/ui/index.ts` that re-exports React components by
// name; this file imports those barrels and exposes a uniform lookup.
//
// For v0.2.x, this is a compile-time bundling — every pack's UI ships
// in the binary regardless of whether the pack is enabled. Disabled
// packs' overrides are never consulted because Manage only iterates
// enabled-pack schemas. Tree-shaking handles the disabled-at-build-time
// case for distros using `--no-default-features`.
//
// Phase 3 (runtime-installable packs) replaces this with dynamic
// imports + a JS sandbox.

import * as l2eUI from "../packs/lead_to_empower/ui";
// The tutoring pack ships no overrides today — every table uses
// auto-CRUD. When it grows custom UI, add an import here.

export type ViewKind = "list" | "detail" | "form";

export type PackUIRegistry = Record<string, ComponentType<UIComponentProps>>;

export type UIComponentProps = {
  // Detail / form views need a row id; list views don't.
  id?: number;
  onClose?: () => void;
};

// Map of pack slug → exported components.
const PACK_UI_BUNDLES: Record<string, PackUIRegistry> = {
  "lead-to-empower": l2eUI as unknown as PackUIRegistry,
};

// Declarations of which (table, view) pair each pack's component
// services. Lives client-side because it's a UI concern; if Rust ever
// needs to know (e.g. for prompt context "the invoice tab is custom"),
// we'll mirror it via PackHandle metadata.
type OverrideDecl = {
  packSlug: string;
  tableSlug: string;
  view: ViewKind;
  /** Component name as exported from the pack's `ui/index.ts`. */
  component: string;
};

const OVERRIDES: OverrideDecl[] = [
  {
    packSlug: "lead-to-empower",
    tableSlug: "invoice",
    view: "list",
    component: "InvoicesTab",
  },
  // v0.20.0 — relationship-aware drill-downs for the three core
  // entities. Engagement = contract (collapsed in pack v0.7.0); the
  // table slug stays `engagement` because the SQL table name didn't
  // change.
  {
    packSlug: "lead-to-empower",
    tableSlug: "school",
    view: "detail",
    component: "SchoolDetail",
  },
  {
    packSlug: "lead-to-empower",
    tableSlug: "engagement",
    view: "detail",
    component: "EngagementDetail",
  },
  {
    packSlug: "lead-to-empower",
    tableSlug: "coach",
    view: "detail",
    component: "CoachDetail",
  },
];

/// Look up a custom UI component for a given (pack, table, view) trio.
/// Returns null if the pack hasn't declared an override — caller falls
/// back to the auto-CRUD component.
export function getOverride(
  packSlug: string,
  tableSlug: string,
  view: ViewKind,
): ComponentType<UIComponentProps> | null {
  const decl = OVERRIDES.find(
    (o) =>
      o.packSlug === packSlug && o.tableSlug === tableSlug && o.view === view,
  );
  if (!decl) return null;
  const bundle = PACK_UI_BUNDLES[decl.packSlug];
  if (!bundle) return null;
  return bundle[decl.component] ?? null;
}
