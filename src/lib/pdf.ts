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
