import { invoke } from "@tauri-apps/api/core";

export type ActionStatus = "proposed" | "confirmed" | "declined" | "applied" | "failed";

export type ProposedAction = {
  id: number;
  conversationId: number;
  kind: string;
  rationale: string | null;
  paramsJson: string;
  status: ActionStatus;
  resultJson: string | null;
  createdAt: string;
  updatedAt: string;
};

export type ProposedActionFilter = {
  conversationId?: number;
  status?: ActionStatus;
};

export const listProposedActions = (filter?: ProposedActionFilter) =>
  invoke<ProposedAction[]>("list_proposed_actions", { filter });

export const confirmAction = (id: number) =>
  invoke<ProposedAction>("confirm_action", { id });

export const declineAction = (id: number) =>
  invoke<ProposedAction>("decline_action", { id });

export function actionLabel(kind: string): string {
  switch (kind) {
    case "defer_task":
      return "Move task";
    case "propose_invoice_draft":
      return "Draft invoice";
    case "set_reminder":
      return "Set reminder";
    case "write_clipboard":
      return "Copy to clipboard";
    case "run_shell_command":
      return "Run on your computer";
    case "send_email":
      return "Send email";
    case "update_profile_context":
      return "Save to your profile";
    // v0.19.5 consent-gated critical changes (v0.20.0 surfaces in chat).
    case "lte_engagement_critical_change":
      return "Change engagement terms";
    case "lte_invoice_critical_change":
      return "Revise invoice";
    default:
      return kind.replace(/_/g, " ");
  }
}

/// Returns a one-line description of params suitable for showing under the
/// rationale, in human terms. Returns null when there's nothing useful to add.
export function actionDetails(kind: string, paramsJson: string): string | null {
  let params: Record<string, unknown> = {};
  try {
    params = JSON.parse(paramsJson);
  } catch {
    return null;
  }
  switch (kind) {
    case "defer_task":
      return params.newDueAt ? `New due date: ${params.newDueAt}` : null;
    case "propose_invoice_draft":
      return [
        params.coachName ? `Coach ${params.coachName}` : null,
        params.schoolName ? params.schoolName : null,
        params.periodStart && params.periodEnd
          ? `${params.periodStart} → ${params.periodEnd}`
          : null,
      ]
        .filter(Boolean)
        .join(" · ") || null;
    case "set_reminder":
      return params.remindAt ? `At ${params.remindAt}` : null;
    case "write_clipboard": {
      const t = String(params.text ?? "");
      const preview = t.length > 80 ? t.slice(0, 80) + "…" : t;
      return preview ? `“${preview}”` : null;
    }
    // For run_shell_command, we deliberately DON'T expose the command in the
    // primary card body — the LLM's rationale is supposed to explain the
    // outcome in plain English. The literal command is available behind the
    // "Show command" detail toggle in the card.
    case "run_shell_command":
      return null;
    case "send_email": {
      const to = String(params.to ?? "");
      const subject = String(params.subject ?? "");
      const provider = String(params.provider ?? "gmail");
      const parts: string[] = [];
      if (to) parts.push(`To ${to}`);
      if (subject) parts.push(`"${subject}"`);
      if (provider && provider !== "gmail") parts.push(`via ${provider}`);
      return parts.join(" · ") || null;
    }
    case "update_profile_context": {
      const blurb = String(params.contextBlurb ?? "").trim();
      const style = String(params.communicationStyle ?? "").trim();
      const parts: string[] = [];
      if (blurb) {
        const preview = blurb.length > 140 ? blurb.slice(0, 140) + "…" : blurb;
        parts.push(`Context: "${preview}"`);
      }
      if (style) parts.push(`Voice: ${style}`);
      return parts.join(" · ") || null;
    }
    case "lte_engagement_critical_change":
    case "lte_invoice_critical_change": {
      const field = String(params.field ?? "");
      const oldV = params.oldValue;
      const newV = params.newValue;
      if (field === "amount_cents" || field === "ceiling_cents") {
        const fmt = (v: unknown) =>
          typeof v === "number" ? `$${(v / 100).toFixed(2)}` : String(v);
        return `${field}: ${fmt(oldV)} → ${fmt(newV)}`;
      }
      return `${field}: ${oldV} → ${newV}`;
    }
    default:
      return null;
  }
}

/// Whether the card should offer a collapsed "Show details" reveal of the
/// raw params. True for technical actions like shell commands and emails
/// (where the user wants to read the body before sending).
export function actionHasTechnicalDetails(kind: string): boolean {
  return kind === "run_shell_command" || kind === "send_email";
}

export function actionTechnicalDetails(
  kind: string,
  paramsJson: string,
): string | null {
  try {
    const params = JSON.parse(paramsJson) as Record<string, unknown>;
    if (kind === "run_shell_command") {
      const lines: string[] = [];
      if (params.command) lines.push(`$ ${params.command}`);
      if (params.workingDir) lines.push(`(in ${params.workingDir})`);
      if (params.timeoutSeconds) lines.push(`(timeout ${params.timeoutSeconds}s)`);
      return lines.join("\n") || null;
    }
    if (kind === "send_email") {
      const lines: string[] = [];
      if (params.to) lines.push(`To: ${params.to}`);
      if (params.subject) lines.push(`Subject: ${params.subject}`);
      if (params.body) {
        lines.push("");
        lines.push(String(params.body));
      }
      return lines.join("\n") || null;
    }
    return null;
  } catch {
    return null;
  }
}
