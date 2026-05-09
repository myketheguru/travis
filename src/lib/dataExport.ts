import { invoke } from "@tauri-apps/api/core";

/// Result of a successful `export_data` call. The frontend renders
/// the path with a "reveal in folder" affordance and surfaces
/// counts + redactions so the user sees what's in the file.
export type ExportResult = {
  path: string;
  sizeBytes: number;
  /// table → row count. Rendered as a small summary so the user can
  /// gauge the volume before sending.
  tableRowCounts: Record<string, number>;
  /// Human-readable notes about what was redacted or skipped.
  redactions: string[];
};

/// Build a JSON export of every user-table row in the current
/// instance and write it to <appData>/exports/. Returns the
/// absolute path. Sensitive workspaces are excluded by default;
/// pass `includeSensitiveWorkspaces=true` for the full picture.
export const exportData = (includeSensitiveWorkspaces = false) =>
  invoke<ExportResult>("export_data", { includeSensitiveWorkspaces });

/// Reveal the export file in the OS file manager so the user can
/// attach it to an email.
export const revealExport = (path: string) =>
  invoke<void>("reveal_export", { path });
