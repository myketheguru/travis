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
// v3 Slice 4 (final) — cloudSignInWithGoogle removed. The handoff
// flow below is the only sign-in path.

/**
 * v3 Slice 4 — pick up an existing web session (or sign up via
 * /app/start in the browser). Opens the user's default browser to
 * the handoff approval page; on approve, the desktop claims the
 * code and ends signed in.
 */
export function cloudHandoffFromWeb(): Promise<CloudUser> {
  return invoke<CloudUser>("cloud_handoff_from_web");
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

// ─── Travis-to-Travis ────────────────────────────────────────────
// v0.22.15 (Shell 6) — light TS wrappers for the attention strip.

export type T2tQueryStatus =
  | "pending"
  | "drafted"
  | "approved"
  | "declined"
  | "answered"
  | "expired";

export interface T2tQuery {
  id: string;
  from_user_id: string;
  to_user_id: string;
  from_conversation_id?: string | null;
  question: string;
  context_json?: string | null;
  status: T2tQueryStatus;
  drafted_response?: string | null;
  drafted_at?: string | null;
  response?: string | null;
  responded_at?: string | null;
  declined_reason?: string | null;
  created_at: string;
  expires_at?: string | null;
  from_email?: string | null;
  from_name?: string | null;
  to_email?: string | null;
  to_name?: string | null;
}

export function t2tInbox(): Promise<T2tQuery[]> {
  return invoke<T2tQuery[]>("t2t_inbox");
}

export function t2tOutbox(): Promise<T2tQuery[]> {
  return invoke<T2tQuery[]>("t2t_outbox");
}

export interface T2tRelationship {
  id: string;
  from_user_id: string;
  to_user_id: string;
  status: "pending" | "active" | "revoked";
  invited_at?: string | null;
  accepted_at?: string | null;
  revoked_at?: string | null;
  reason?: string | null;
  other_email?: string | null;
  other_name?: string | null;
}

export function t2tListRelationships(): Promise<T2tRelationship[]> {
  return invoke<T2tRelationship[]>("t2t_list_relationships");
}

export function t2tInvite(email: string, reason?: string): Promise<string> {
  return invoke<string>("t2t_invite", { email, reason: reason ?? null });
}

export function t2tAccept(id: string): Promise<void> {
  return invoke<void>("t2t_accept", { id });
}

export function t2tRevoke(id: string, reason?: string): Promise<void> {
  return invoke<void>("t2t_revoke", { id, reason: reason ?? null });
}

// v0.28.46 — pair tokens (QR / deep-link pairing beyond LAN).

export interface PairToken {
  token: string;
  expires_at: string;
  deep_link: string;
}

export interface PairRedeemResult {
  ok: boolean;
  relationship_id: string;
  other_user: {
    id: string;
    name: string | null;
    email: string;
  } | null;
}

export function t2tPairCreateToken(): Promise<PairToken> {
  return invoke<PairToken>("t2t_pair_create_token");
}

export function t2tPairRedeem(token: string): Promise<PairRedeemResult> {
  return invoke<PairRedeemResult>("t2t_pair_redeem", { token });
}

// v0.28.48 — Circles: named groups for beyond-LAN Travis discovery.

export interface Circle {
  id: string;
  name: string;
  description: string | null;
  join_code: string;
  creator_id: string | null;
  created_at: string | null;
  role: string;
  member_count: number;
}

export interface CircleMember {
  id: string;
  name: string | null;
  email: string;
  role: string;
  joined_at: string;
}

export interface CircleContact {
  id: string;
  name: string | null;
  email: string;
}

export interface CircleJoinResult {
  id: string;
  name: string;
  description: string | null;
  role: string;
  already_member: boolean;
}

export function circlesCreate(name: string, description?: string): Promise<Circle> {
  return invoke<Circle>("circles_create", { name, description: description ?? null });
}

export function circlesList(): Promise<Circle[]> {
  return invoke<Circle[]>("circles_list");
}

export function circlesJoin(code: string): Promise<CircleJoinResult> {
  return invoke<CircleJoinResult>("circles_join", { code });
}

export function circlesLeave(id: string): Promise<void> {
  return invoke<void>("circles_leave", { id });
}

export function circlesMembers(id: string): Promise<CircleMember[]> {
  return invoke<CircleMember[]>("circles_members", { id });
}

export function circlesContacts(): Promise<CircleContact[]> {
  return invoke<CircleContact[]>("circles_contacts");
}

export function circlesDelete(id: string): Promise<void> {
  return invoke<void>("circles_delete", { id });
}

// v0.28.49 — BLE scaffold. scan/advertise/send-file return the empty
// v0.28.49 placeholder shape today; v0.28.50 wires the real btleplug
// impl behind these same signatures.

export interface BlePeer {
  instance_id: string;
  display_name: string | null;
  user_id: string | null;
  public_key: string | null;
  rssi: number | null;
  last_seen: number;
}

export function bleScanPeers(): Promise<BlePeer[]> {
  return invoke<BlePeer[]>("ble_scan_peers");
}

export function bleStartAdvertise(
  displayName: string,
  userId?: string,
  publicKey?: string,
): Promise<void> {
  return invoke<void>("ble_start_advertise", {
    displayName,
    userId: userId ?? null,
    publicKey: publicKey ?? null,
  });
}

export function bleSendFile(peerInstanceId: string, path: string): Promise<string> {
  return invoke<string>("ble_send_file", { peerInstanceId, path });
}

export function t2tSendQuery(
  toUserId: string,
  question: string,
  fromConversationId?: string,
  expiresAfterDays?: number,
): Promise<string> {
  return invoke<string>("t2t_send_query", {
    toUserId,
    question,
    fromConversationId: fromConversationId ?? null,
    expiresAfterDays: expiresAfterDays ?? null,
  });
}

export function t2tDraftReply(id: string, draftedResponse: string): Promise<void> {
  return invoke<void>("t2t_draft_reply", { id, draftedResponse });
}

export function t2tApproveReply(id: string, finalResponse?: string): Promise<void> {
  return invoke<void>("t2t_approve_reply", {
    id,
    finalResponse: finalResponse ?? null,
  });
}

export function t2tDeclineReply(id: string, reason?: string): Promise<void> {
  return invoke<void>("t2t_decline_reply", { id, reason: reason ?? null });
}

/** v0.24 task 311 slice B — desktop-side auto-draft. Reads the query
 *  from inbox, calls the local LLM to draft a short reply, POSTs it
 *  via t2t_draft_reply. Skipped if a non-empty draft already exists. */
export function t2tAutoDraft(queryId: string): Promise<string> {
  return invoke<string>("t2t_auto_draft", { queryId });
}

// ─── MCP (task 313) ──────────────────────────────────────────────

export interface McpServer {
  id: number;
  slug: string;
  label: string;
  url: string;
  auth_token: string | null;
  enabled: boolean;
  created_at: string;
}

export function mcpListServers(): Promise<McpServer[]> {
  return invoke<McpServer[]>("mcp_list_servers");
}

export function mcpAddServer(
  slug: string,
  label: string,
  url: string,
  authToken?: string,
): Promise<number> {
  return invoke<number>("mcp_add_server", {
    slug,
    label,
    url,
    authToken: authToken ?? null,
  });
}

export function mcpDeleteServer(id: number): Promise<void> {
  return invoke<void>("mcp_delete_server", { id });
}

export function mcpSetEnabled(id: number, enabled: boolean): Promise<void> {
  return invoke<void>("mcp_set_enabled", { id, enabled });
}

export function mcpPingServer(url: string, authToken?: string): Promise<string[]> {
  return invoke<string[]>("mcp_ping_server", {
    url,
    authToken: authToken ?? null,
  });
}

// ─── Peer discovery (task 314) ───────────────────────────────────

export interface DiscoveredPeer {
  instance_name: string;
  display_name?: string | null;
  user_email?: string | null;
  user_id?: string | null;
  host: string;
  port: number;
  last_seen: number;
}

export function discoveryStart(): Promise<void> {
  return invoke<void>("discovery_start");
}

export function discoveryPeers(): Promise<DiscoveredPeer[]> {
  return invoke<DiscoveredPeer[]>("discovery_peers");
}

// ─── Sentry (task 315) ───────────────────────────────────────────

export interface SentryStatus {
  enabled: boolean;
  buffered: number;
  snapshot_count: number;
  snapshot_bytes: number;
}

export interface SentrySnapshotInfo {
  path: string;
  filename: string;
  captured_at: string;
  bytes: number;
}

export function sentryStatus(): Promise<SentryStatus> {
  return invoke<SentryStatus>("sentry_status");
}

export function sentryListSnapshots(limit?: number): Promise<SentrySnapshotInfo[]> {
  return invoke<SentrySnapshotInfo[]>("sentry_list_snapshots", { limit });
}

export function sentryCaptureNow(): Promise<SentrySnapshotInfo> {
  return invoke<SentrySnapshotInfo>("sentry_capture_now");
}

// ─── T2T secure file transfer (v0.28.53) ─────────────────────────

export interface T2tInboxFile {
  id: string;
  from_user_id: string;
  from_email?: string;
  from_name?: string;
  filename: string;
  content_type?: string;
  ciphertext_bytes: number;
  sender_ephem_pub: string;
  created_at: string;
}

export function t2tPublishPubkey(): Promise<void> {
  return invoke<void>("t2t_publish_pubkey");
}

export function t2tSendFile(
  peerId: string,
  filePath: string,
): Promise<string> {
  return invoke<string>("t2t_send_file", { peerId, filePath });
}

export function t2tPollInbox(): Promise<T2tInboxFile[]> {
  return invoke<T2tInboxFile[]>("t2t_poll_inbox");
}

export function t2tReceiveFile(transferId: string): Promise<string> {
  return invoke<string>("t2t_receive_file", { transferId });
}

export function sentrySetEnabled(enabled: boolean): Promise<void> {
  return invoke<void>("sentry_set_enabled", { enabled });
}
