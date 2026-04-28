import { invoke } from "@tauri-apps/api/core";

export type DbStats = {
  coaches: number;
  schools: number;
  coachHours: number;
  signingSheets: number;
  invoices: number;
  tasksOpen: number;
  tasksTotal: number;
};

export type Coach = {
  id: number;
  name: string;
  email: string | null;
  rateCents: number | null;
  notes: string | null;
  createdAt: string;
  updatedAt: string;
};

export type CoachInput = {
  id?: number;
  name: string;
  email?: string | null;
  rateCents?: number | null;
  notes?: string | null;
};

export type School = {
  id: number;
  name: string;
  district: string | null;
  contactName: string | null;
  contactEmail: string | null;
  notes: string | null;
  createdAt: string;
  updatedAt: string;
};

export type SchoolInput = {
  id?: number;
  name: string;
  district?: string | null;
  contactName?: string | null;
  contactEmail?: string | null;
  notes?: string | null;
};

export type CoachHours = {
  id: number;
  coachId: number;
  schoolId: number;
  sessionDate: string;
  hours: number;
  description: string | null;
  createdAt: string;
  updatedAt: string;
};

export type CoachHoursInput = {
  id?: number;
  coachId: number;
  schoolId: number;
  sessionDate: string;
  hours: number;
  description?: string | null;
};

export type CoachHoursFilter = {
  coachId?: number;
  schoolId?: number;
  periodStart?: string;
  periodEnd?: string;
};

export type SigningSheet = {
  id: number;
  coachId: number;
  schoolId: number;
  periodStart: string;
  periodEnd: string;
  signedAt: string | null;
  signedBy: string | null;
  pdfPath: string | null;
  notes: string | null;
  createdAt: string;
  updatedAt: string;
};

export type SigningSheetInput = {
  id?: number;
  coachId: number;
  schoolId: number;
  periodStart: string;
  periodEnd: string;
  signedAt?: string | null;
  signedBy?: string | null;
  pdfPath?: string | null;
  notes?: string | null;
};

export type SigningSheetFilter = {
  coachId?: number;
  schoolId?: number;
  signed?: boolean;
};

export type InvoiceStatus = "draft" | "sent" | "paid" | "void";

export type Invoice = {
  id: number;
  number: string;
  recipient: string;
  coachId: number | null;
  schoolId: number | null;
  signingSheetId: number | null;
  periodStart: string;
  periodEnd: string;
  hoursTotal: number;
  rateCents: number;
  amountCents: number;
  status: InvoiceStatus;
  issuedAt: string | null;
  paidAt: string | null;
  notes: string | null;
  createdAt: string;
  updatedAt: string;
};

export type InvoiceInput = {
  id?: number;
  number: string;
  recipient: string;
  coachId?: number | null;
  schoolId?: number | null;
  signingSheetId?: number | null;
  periodStart: string;
  periodEnd: string;
  hoursTotal: number;
  rateCents: number;
  notes?: string | null;
};

export type InvoiceFilter = {
  status?: InvoiceStatus;
  coachId?: number;
};

export type TaskStatus = "open" | "done" | "snoozed" | "dropped";

export type Task = {
  id: number;
  title: string;
  description: string | null;
  status: TaskStatus;
  priority: number;
  dueAt: string | null;
  linkKind: string | null;
  linkId: number | null;
  source: string;
  completedAt: string | null;
  createdAt: string;
  updatedAt: string;
};

export type TaskInput = {
  id?: number;
  title: string;
  description?: string | null;
  priority?: number;
  dueAt?: string | null;
  linkKind?: string | null;
  linkId?: number | null;
  source?: string;
};

export type TaskFilter = {
  status?: TaskStatus;
  linkKind?: string;
  linkId?: number;
};

export const dbStats = () => invoke<DbStats>("db_stats");

export const listCoaches = () => invoke<Coach[]>("list_coaches");
export const upsertCoach = (input: CoachInput) => invoke<Coach>("upsert_coach", { input });
export const deleteCoach = (id: number) => invoke<void>("delete_coach", { id });

export const listSchools = () => invoke<School[]>("list_schools");
export const upsertSchool = (input: SchoolInput) => invoke<School>("upsert_school", { input });
export const deleteSchool = (id: number) => invoke<void>("delete_school", { id });

export const listCoachHours = (filter?: CoachHoursFilter) =>
  invoke<CoachHours[]>("list_coach_hours", { filter });
export const logCoachHours = (input: CoachHoursInput) =>
  invoke<CoachHours>("log_coach_hours", { input });
export const deleteCoachHours = (id: number) => invoke<void>("delete_coach_hours", { id });

export const listSigningSheets = (filter?: SigningSheetFilter) =>
  invoke<SigningSheet[]>("list_signing_sheets", { filter });
export const upsertSigningSheet = (input: SigningSheetInput) =>
  invoke<SigningSheet>("upsert_signing_sheet", { input });
export const deleteSigningSheet = (id: number) => invoke<void>("delete_signing_sheet", { id });

export const listInvoices = (filter?: InvoiceFilter) =>
  invoke<Invoice[]>("list_invoices", { filter });
export const upsertInvoice = (input: InvoiceInput) => invoke<Invoice>("upsert_invoice", { input });
export const transitionInvoice = (id: number, status: InvoiceStatus) =>
  invoke<Invoice>("transition_invoice", { id, status });
export const deleteInvoice = (id: number) => invoke<void>("delete_invoice", { id });

export const listTasks = (filter?: TaskFilter) => invoke<Task[]>("list_tasks", { filter });
export const upsertTask = (input: TaskInput) => invoke<Task>("upsert_task", { input });
export const setTaskStatus = (id: number, status: TaskStatus) =>
  invoke<Task>("set_task_status", { id, status });
export const deleteTask = (id: number) => invoke<void>("delete_task", { id });
