/**
 * Cloud bindings — Phase 1 of the v2 cloud-first architecture.
 *
 * Wraps the Tauri commands defined in `src-tauri/src/cloud/cmd.rs`.
 * Every backend interaction goes through here so we have a single
 * place to handle 401s and reroute the user to sign-in.
 */
import { invoke } from "@tauri-apps/api/core";

export interface CloudUser {
  id: string;
  email: string;
  name: string | null;
  orgId: string | null;
  tier: string;
}

export interface CloudStatus {
  signedIn: boolean;
  user: CloudUser | null;
  invalidToken: boolean;
}

export interface CloudPolicyUsage {
  calls: number;
  costCents: number;
}

export interface CloudPolicy {
  tier: string;
  allowedModels: string[];
  dailyCallCap: number;
  dailyCostCapCents: number;
  usedToday: CloudPolicyUsage;
  remainingToday: CloudPolicyUsage;
}

export interface ByokEvent {
  model: string;
  provider: "anthropic" | "openai" | "ollama" | "google";
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens?: number;
  cacheWriteTokens?: number;
  context?: Record<string, unknown>;
}

/**
 * Check whether a session is active and the JWT still works. Resolves
 * with `signedIn: false, invalidToken: true` if a stale token was found
 * and cleared — the UI should prompt for re-sign-in.
 */
export function cloudStatus(): Promise<CloudStatus> {
  return invoke<CloudStatus>("cloud_status");
}

/**
 * Fast synchronous-ish check for whether a JWT exists in the keychain
 * at all, without hitting the network. Useful for first-paint gating.
 */
export function cloudHasToken(): Promise<boolean> {
  return invoke<boolean>("cloud_has_token");
}

/**
 * Drive the full Google sign-in flow. Resolves with the new user
 * profile when sign-in completes successfully. Rejects with a string
 * message on failure (timeout, user closed the tab, network error).
 *
 * The user's browser will open during this call. Do NOT call from a
 * loop or auto-retry on failure — that would spawn an infinite series
 * of browser tabs.
 */
export function cloudSignInWithGoogle(): Promise<CloudUser> {
  return invoke<CloudUser>("cloud_sign_in_with_google");
}

/**
 * Abort the in-flight Google sign-in. The cloudSignInWithGoogle()
 * promise rejects with "sign-in canceled". Idempotent — safe to call
 * when no sign-in is running.
 */
export function cloudSignInCancel(): Promise<void> {
  return invoke<void>("cloud_sign_in_cancel");
}

/**
 * Tier 2 — extend the signed-in user's Google grant to add the read
 * scopes Travis needs to power inbox triage + calendar context.
 *
 * Pass scope keys: 'gmail' for gmail.readonly, 'gcal' for
 * calendar.readonly. Opens the browser to the consent screen and
 * resolves with a comma-separated list of providers enrolled.
 */
export function cloudExtendGoogleGrant(scopes: ('gmail' | 'gcal')[]): Promise<string> {
  return invoke<string>('cloud_extend_google_grant', { scopes });
}

/**
 * Sign out — clears the JWT locally and tells the backend to revoke it.
 * The local clear happens even if the backend round-trip fails.
 */
export function cloudSignOut(): Promise<void> {
  return invoke<void>("cloud_sign_out");
}

/**
 * Fetch the user's current tier policy + today's usage. Throws if not
 * signed in. The desktop should call this on app launch, on tier
 * change events, and periodically (e.g. every 5 min while active).
 */
export function cloudPolicy(): Promise<CloudPolicy> {
  return invoke<CloudPolicy>("cloud_policy");
}

/**
 * Report a BYOK LLM call to the backend so usage stays identity-tagged.
 * Best-effort — never throws on the desktop side.
 */
export function cloudRecordByok(event: ByokEvent): Promise<void> {
  return invoke<void>("cloud_record_byok", { event });
}

// --- v2 Phase 2.1 — migration -----------------------------------------

export interface LocalCounts {
  profile: number;
  memories: number;
  conversations: number;
  conversationMessages: number;
  settings: number;
}

export interface MigrationDetails {
  pushed: number;
  skipped: number;
  at: string;
  decision: string;
}

export interface MigrationStatus {
  /** "" if undecided, "complete" | "fresh" | "skipped" otherwise. */
  status: string;
  localCounts: LocalCounts;
  details: MigrationDetails | null;
}

export function cloudMigrationStatus(): Promise<MigrationStatus> {
  return invoke<MigrationStatus>("cloud_migration_status");
}

export function cloudMigrationUpload(): Promise<MigrationDetails> {
  return invoke<MigrationDetails>("cloud_migration_upload");
}

export function cloudMigrationStartFresh(): Promise<void> {
  return invoke<void>("cloud_migration_start_fresh");
}

export function cloudMigrationSkip(): Promise<void> {
  return invoke<void>("cloud_migration_skip");
}

// --- v2 Phase 2.2 — continuous sync -----------------------------------

export interface SyncStatus {
  cursor: string;
  pendingOutbox: number;
  failingOutbox: number;
  pendingFiles: number;
  lastSyncAt: string | null;
  lastError: string | null;
}

export interface SyncRunResult {
  pushed: number;
  pulledApplied: number;
  pulledSkipped: number;
  filesUploaded: number;
  cursor: string;
}

/**
 * Trigger an immediate push + pull cycle. Settings exposes a "Sync now"
 * button that calls this. Safe to call frequently — no-ops cleanly if
 * there's nothing to do.
 */
export function cloudSyncNow(): Promise<SyncRunResult> {
  return invoke<SyncRunResult>("cloud_sync_now");
}

/**
 * Read current sync state — cursor, pending outbox count, last sync
 * timestamp, last error. Cheap; safe to poll.
 */
export function cloudSyncStatus(): Promise<SyncStatus> {
  return invoke<SyncStatus>("cloud_sync_status");
}

// --- v2 Phase 4 — workflow loop ---------------------------------------

export interface WorkflowSchedule {
  id: string;
  name: string;
  trigger_kind: string;
  trigger_spec: string;
  prompt: string;
  is_active: number;
  created_at?: string;
  updated_at?: string;
}

export interface WorkflowRun {
  id: string;
  user_id: string;
  schedule_id?: string | null;
  schedule_name?: string | null;
  source: string;
  status: string;
  started_at: string;
  finished_at?: string | null;
  result_text?: string | null;
  /** v0.21.5 Tier 3 — JSON-encoded array of ProposedAction. */
  result_actions_json?: string | null;
  input_tokens: number;
  output_tokens: number;
  cost_usd_cents: number;
  error_message?: string | null;
}

/** v0.21.5 Tier 3 — one action a workflow run wants the user to approve. */
export interface ProposedAction {
  id: string;
  kind: "draft_reply" | string;
  payload: {
    to?: string;
    subject?: string;
    body?: string;
    context?: Record<string, unknown>;
    [k: string]: unknown;
  };
  status: "pending" | "approved" | "executed" | "discarded" | "edited";
  updated_at?: string;
}

/** Update a single action's status on a workflow run. The desktop
 *  calls this after user clicks Approve / Discard / Edit-then-Send. */
export function cloudUpdateActionStatus(
  runId: string,
  actionId: string,
  status: ProposedAction["status"],
  payload?: ProposedAction["payload"],
): Promise<void> {
  return invoke<void>("cloud_update_action_status", {
    runId,
    actionId,
    status,
    payload: payload ?? null,
  });
}

/** v0.21.5 Tier 3 — execute an approved draft_reply via local Gmail
 *  OAuth and mark the action as executed on the cloud run. */
export function cloudActionExecuteDraftReply(
  runId: string,
  actionId: string,
  to: string,
  subject: string,
  body: string,
): Promise<void> {
  return invoke<void>("cloud_action_execute_draft_reply", {
    runId,
    actionId,
    to,
    subject,
    body,
  });
}

export interface ConnectedAccount {
  provider: string;
  scopes_granted: string;
  provider_account_id: string | null;
  is_active: number;
  created_at: string;
  last_used_at: string | null;
}

export function cloudConnectedAccounts(): Promise<ConnectedAccount[]> {
  return invoke<ConnectedAccount[]>("cloud_connected_accounts");
}

export function cloudDisconnectAccount(provider: string): Promise<void> {
  return invoke<void>("cloud_disconnect_account", { provider });
}

export interface CreateScheduleInput {
  name: string;
  // v0.21.6 — 'inbox_summary' added so the cloud WorkflowLoop branches
  // into the inbox-read pipeline instead of the agent loop.
  triggerKind: "cron" | "calendar" | "email_match" | "manual" | "inbox_summary";
  triggerSpec: Record<string, unknown>;
  prompt: string;
  isActive: boolean;
}

export interface RunNowInput {
  scheduleId?: string;
  prompt?: string;
}

export function cloudWorkflowSchedules(): Promise<WorkflowSchedule[]> {
  return invoke<WorkflowSchedule[]>("cloud_workflow_schedules");
}

export function cloudWorkflowCreateSchedule(
  input: CreateScheduleInput,
): Promise<string> {
  return invoke<string>("cloud_workflow_create_schedule", { input });
}

/** v0.21.6 — convenience helper to enroll a default hourly inbox
 *  summary schedule after a successful Connect on Inbox. Looks up
 *  any existing inbox_summary schedule first and no-ops if one
 *  already exists, so calling repeatedly is safe. */
export async function ensureInboxSummarySchedule(): Promise<string | null> {
  try {
    const existing = await cloudWorkflowSchedules();
    const already = existing.find((s) => s.trigger_kind === "inbox_summary");
    if (already) return already.id;
  } catch {
    /* fall through and try to create — better to risk a duplicate than
       block the happy path on a list failure */
  }
  return cloudWorkflowCreateSchedule({
    name: "Inbox summary",
    triggerKind: "inbox_summary",
    triggerSpec: { intervalMinutes: 60 },
    prompt:
      "Summarize the user's new inbox messages since the last check. Identify what's urgent, what's notable, and what can be ignored. Draft replies for urgent items where the action is clear.",
    isActive: true,
  });
}

export function cloudWorkflowDeleteSchedule(id: string): Promise<void> {
  return invoke<void>("cloud_workflow_delete_schedule", { id });
}

export function cloudWorkflowRunNow(input: RunNowInput): Promise<string> {
  return invoke<string>("cloud_workflow_run_now", { input });
}

export function cloudWorkflowRuns(since?: string): Promise<WorkflowRun[]> {
  return invoke<WorkflowRun[]>("cloud_workflow_runs", { since: since ?? null });
}
