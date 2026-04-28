import { invoke } from "@tauri-apps/api/core";
import type { AppStatus } from "../stores/app";

export type Provider = "claude" | "openai" | "ollama";

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
};

export async function getProactiveConfig(): Promise<ProactiveConfig> {
  return invoke<ProactiveConfig>("get_proactive_config");
}

export async function setProactiveEnabled(enabled: boolean): Promise<void> {
  await invoke("set_proactive_enabled", { enabled });
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
