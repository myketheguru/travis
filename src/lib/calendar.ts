import { invoke } from "@tauri-apps/api/core";

export type ConnectionStatus = {
  provider: string;
  connected: boolean;
  accountId: string | null;
  scopes: string[];
  connectedAt: string | null;
  expiresAt: string | null;
  /// True when the build has the OAuth client credentials baked in. False on
  /// dev builds without TRAVIS_GOOGLE_CLIENT_ID/SECRET set — Connect won't
  /// work in that case.
  configured: boolean;
};

export const calendarStatus = () => invoke<ConnectionStatus>("calendar_status");

export const calendarConnectGoogle = () =>
  invoke<string>("calendar_connect_google");

export const calendarDisconnectGoogle = () =>
  invoke<void>("calendar_disconnect_google");

export const microsoftStatus = () => invoke<ConnectionStatus>("microsoft_status");

export const microsoftConnect = () => invoke<string>("microsoft_connect");

export const microsoftDisconnect = () => invoke<void>("microsoft_disconnect");
