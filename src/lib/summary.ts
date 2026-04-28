import { invoke } from "@tauri-apps/api/core";

export type SummaryKind = "daily" | "weekly";

export type Summary = {
  id: number;
  kind: SummaryKind;
  periodStart: string;
  periodEnd: string;
  content: string;
  sourceCount: number;
  model: string | null;
  provider: string | null;
  createdAt: string;
};

export const listSummaries = (kind?: SummaryKind, limit?: number) =>
  invoke<Summary[]>("list_summaries", { kind, limit });

export const generateDailySummary = (date: string) =>
  invoke<Summary>("generate_daily_summary", { date });

export const generateWeeklySummary = (weekStart: string) =>
  invoke<Summary>("generate_weekly_summary", { weekStart });
