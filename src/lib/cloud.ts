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
