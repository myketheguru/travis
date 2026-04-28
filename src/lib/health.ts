import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type IssueKind =
  | "offline"
  | "quotaExhausted"
  | "rateLimited"
  | "unauthorized"
  | "serverError"
  | "networkError"
  | "provider";

export type Issue = {
  kind: IssueKind;
  message: string;
  since: string;
};

export type HealthState = {
  online: boolean;
  issue: Issue | null;
};

export const healthStatus = () => invoke<HealthState>("health_status");

export const healthSetOnline = (online: boolean) =>
  invoke<void>("health_set_online", { online });

export const healthDismiss = () => invoke<void>("health_dismiss");

/** Subscribe to backend-driven health changes. Returns an unlisten fn. */
export const onHealthChanged = (cb: (s: HealthState) => void): Promise<UnlistenFn> =>
  listen<HealthState>("health-changed", (e) => cb(e.payload));
