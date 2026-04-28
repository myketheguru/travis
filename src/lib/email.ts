import { invoke } from "@tauri-apps/api/core";

export type SmtpConfig = {
  id: number;
  host: string;
  port: number;
  username: string;
  fromAddress: string;
  fromName: string | null;
  /** Stored as 0/1 in SQLite, returned as a number; 0 = false, anything else = true. */
  useTls: number;
  updatedAt: string;
};

export type SmtpConfigInput = {
  host: string;
  port?: number;
  username: string;
  fromAddress: string;
  fromName?: string | null;
  useTls?: boolean;
};

export type EmailStatus = "pending" | "sent" | "failed";

export type EmailSent = {
  id: number;
  recipient: string;
  subject: string;
  bodyPreview: string | null;
  kind: string | null;
  relatedKind: string | null;
  relatedId: number | null;
  status: EmailStatus;
  errorMessage: string | null;
  sentAt: string | null;
  createdAt: string;
};

export const getSmtpConfig = () => invoke<SmtpConfig | null>("get_smtp_config");

export const setSmtpConfig = (input: SmtpConfigInput, password?: string) =>
  invoke<SmtpConfig>("set_smtp_config", { input, password: password ?? null });

export const listEmailsSent = (limit?: number) =>
  invoke<EmailSent[]>("list_emails_sent", { limit });

export const sendInvoiceEmail = (
  invoiceId: number,
  recipient: string,
  customMessage?: string,
) =>
  invoke<EmailSent>("send_invoice_email", {
    invoiceId,
    recipient,
    customMessage: customMessage ?? null,
  });

/**
 * Send a plain-text email via the user's connected Gmail account. Requires
 * the Google connection in Settings to include the gmail.send scope.
 */
export const sendEmailGmail = (
  to: string,
  subject: string,
  body: string,
  options?: { relatedKind?: string; relatedId?: number },
) =>
  invoke<EmailSent>("send_email_gmail", {
    to,
    subject,
    body,
    relatedKind: options?.relatedKind ?? null,
    relatedId: options?.relatedId ?? null,
  });

/**
 * Send a plain-text email via the user's connected Microsoft account
 * (Outlook). Requires the Microsoft connection in Settings to include the
 * Mail.Send scope.
 */
export const sendEmailOutlook = (
  to: string,
  subject: string,
  body: string,
  options?: { relatedKind?: string; relatedId?: number },
) =>
  invoke<EmailSent>("send_email_outlook", {
    to,
    subject,
    body,
    relatedKind: options?.relatedKind ?? null,
    relatedId: options?.relatedId ?? null,
  });
