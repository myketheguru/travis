// Component registry for the Lead to Empower pack's UI overrides.
//
// The frontend pack registry (src/lib/packRegistry.ts) imports this and
// looks up components by name. When the L2E pack declares an override
// for `(invoice, list)` in its manifest, the registry resolves the
// `InvoicesTab` export here instead of rendering the auto-CRUD ListView.

export { default as InvoicesTab } from "./InvoicesTab";
