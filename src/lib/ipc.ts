import { invoke } from "@tauri-apps/api/core";
import type { AppStatus } from "../stores/app";

export type Provider = "travis_cloud" | "claude" | "openai" | "ollama";

/// v0.20.2 — info about what this build of Travis supports.
export type PlatformInfo = {
  travisCloudAvailable: boolean;
  previousLlmProvider: string | null;
  previousModel: string | null;
};

/// v0.20.2 — query the build's platform support.
export const platformInfo = () => invoke<PlatformInfo>("platform_info");

/// v0.20.2 — Travis admin can mark a min-supported version remotely; older
/// builds are gated until they upgrade.
export type ForceUpgradeStatus = {
  required: boolean;
  currentVersion: string;
  minVersion: string | null;
  latestVersion: string | null;
  reason: string | null;
};

export const checkForceUpgrade = () =>
  invoke<ForceUpgradeStatus>("check_force_upgrade");

export type UserProfile = {
  name: string;
  role: string;
  org: string;
  llmProvider: Provider;
  ollamaUrl: string | null;
  model: string | null;
  /** Free-form description of the user/org's work — used to template prompts. */
  contextBlurb: string | null;
  /** Optional voice/tone preference for Travis's replies. */
  communicationStyle: string | null;
};

export type OnboardingPayload = {
  name: string;
  role: string;
  org: string;
  provider: Provider;
  apiKey?: string;
  ollamaUrl?: string;
  model?: string;
  contextBlurb?: string;
  communicationStyle?: string;
};

export async function getAppStatus(): Promise<AppStatus> {
  return invoke<AppStatus>("app_status");
}

export async function getUserProfile(): Promise<UserProfile | null> {
  return invoke<UserProfile | null>("get_user_profile");
}

export async function completeOnboarding(payload: OnboardingPayload): Promise<void> {
  await invoke("complete_onboarding", { payload });
}

export async function updateProfile(payload: OnboardingPayload): Promise<void> {
  await invoke("update_profile", { payload });
}

export async function hasApiKey(provider: Provider): Promise<boolean> {
  return invoke<boolean>("has_api_key", { provider });
}

export async function setApiKey(provider: Provider, key: string): Promise<void> {
  await invoke("set_api_key", { provider, key });
}

export async function getShellEnabled(): Promise<boolean> {
  return invoke<boolean>("get_shell_enabled");
}

export async function setShellEnabled(enabled: boolean): Promise<void> {
  await invoke("set_shell_enabled", { enabled });
}

export type ProactiveConfig = {
  enabled: boolean;
  lastAt: string | null;
  /** ISO weekday numbers — Mon=1..Sun=7. */
  activeDays: number[];
  startHour: number;
  endHour: number;
};

export type ProactiveSchedulePayload = {
  activeDays: number[];
  startHour: number;
  endHour: number;
};

export async function getProactiveConfig(): Promise<ProactiveConfig> {
  return invoke<ProactiveConfig>("get_proactive_config");
}

export async function setProactiveEnabled(enabled: boolean): Promise<void> {
  await invoke("set_proactive_enabled", { enabled });
}

export async function setProactiveSchedule(payload: ProactiveSchedulePayload): Promise<void> {
  await invoke("set_proactive_schedule", { payload });
}

export type PingResult = {
  ok: boolean;
  model: string;
  latencyMs: number;
  message: string | null;
};

export type TestProviderPayload = {
  provider: Provider;
  apiKey?: string;
  ollamaUrl?: string;
  model?: string;
};

export async function testProvider(payload: TestProviderPayload): Promise<PingResult> {
  return invoke<PingResult>("test_provider", { payload });
}

export type ChatRole = "system" | "user" | "assistant";
export type ChatMessage = { role: ChatRole; content: string };
export type ChatOptions = {
  system?: string;
  maxTokens?: number;
  temperature?: number;
  cacheSystem?: boolean;
  jsonMode?: boolean;
};
export type ChatResponse = {
  content: string;
  model: string;
  inputTokens: number | null;
  outputTokens: number | null;
  cacheReadTokens: number | null;
};

export async function chat(messages: ChatMessage[], options: ChatOptions = {}): Promise<ChatResponse> {
  return invoke<ChatResponse>("chat", { payload: { messages, options } });
}
