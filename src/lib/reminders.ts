import { invoke } from "@tauri-apps/api/core";

export type ReminderKind = "time" | "context" | "behavioral";

export type Reminder = {
  id: number;
  text: string;
  kind: ReminderKind;
  remindAt: string | null;
  firedAt: string | null;
  dismissedAt: string | null;
  source: string;
  linkKind: string | null;
  linkId: number | null;
  createdAt: string;
  updatedAt: string;
};

export type ReminderInput = {
  id?: number;
  text: string;
  kind?: ReminderKind;
  remindAt?: string | null;
  source?: string;
  linkKind?: string | null;
  linkId?: number | null;
};

export type ReminderFilter = {
  fired?: boolean;
  dismissed?: boolean;
  kind?: ReminderKind;
};

export const listReminders = (filter?: ReminderFilter) =>
  invoke<Reminder[]>("list_reminders", { filter });

export const createReminder = (input: ReminderInput) =>
  invoke<Reminder>("create_reminder", { input });

export const dismissReminder = (id: number) =>
  invoke<Reminder>("dismiss_reminder", { id });

export const deleteReminder = (id: number) =>
  invoke<void>("delete_reminder", { id });
