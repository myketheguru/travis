import { invoke } from "@tauri-apps/api/core";

/**
 * Export an invoice to PDF at `destPath`. The path must be non-empty and
 * absolute — the frontend is responsible for picking it (e.g. via the Tauri
 * `dialog` plugin's save dialog or by joining `appDataDir/invoices/...`).
 *
 * Returns the actual on-disk path the file was written to.
 */
export const exportInvoicePdf = (invoiceId: number, destPath: string) =>
  invoke<string>("export_invoice_pdf", { invoiceId, destPath });

/**
 * Render the invoice to a managed cache path and return the absolute path.
 * The backend chooses the location (under app cache dir / invoices /) and
 * ensures the directory exists. Useful for "view PDF" flows where the user
 * doesn't need to pick a destination.
 */
export const exportInvoicePdfPreview = (invoiceId: number) =>
  invoke<string>("export_invoice_pdf_preview", { invoiceId });

/**
 * Render a Work Order PDF to the user's Downloads folder. Returns the
 * absolute path. Backend handles the destination — frontend doesn't
 * pick a path.
 */
export const exportWorkOrderPdf = (workOrderId: number) =>
  invoke<string>("export_work_order_pdf", { workOrderId });

/**
 * Render a Sign-in Sheet PDF for an engagement + period to the user's
 * Downloads folder. Returns the absolute path.
 */
export const exportSignInSheetPdf = (
  engagementId: number,
  periodStart: string,
  periodEnd: string,
) =>
  invoke<string>("export_sign_in_sheet_pdf", {
    engagementId,
    periodStart,
    periodEnd,
  });
