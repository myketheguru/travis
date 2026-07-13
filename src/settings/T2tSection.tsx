/**
 * T2tSection — Travis-to-Travis relationships management.
 *
 * Lives inside Settings. Lets the user:
 *   - Invite another Travis by email (creates pending relationship
 *     on cloud; other side sees invite in their inbox + accepts)
 *   - Accept incoming pending invites
 *   - Revoke an active or pending relationship
 *
 * Once a relationship is `active`, the user can address the other
 * Travis via natural language ("ask Taylor about X") and the LLM
 * resolves via t2t_list_contacts + calls t2t_ask.
 */
import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  t2tAccept,
  t2tInvite,
  t2tListRelationships,
  t2tRevoke,
  type T2tRelationship,
} from "../lib/cloud";

interface Props {
  /** Current user's cloud id — used to distinguish outgoing from
   *  incoming invites in the relationship list. */
  currentUserId?: string;
}

export function T2tSection({ currentUserId }: Props) {
  const [relationships, setRelationships] = useState<T2tRelationship[]>([]);
  const [loading, setLoading] = useState(true);
  const [inviteEmail, setInviteEmail] = useState("");
  const [inviteReason, setInviteReason] = useState("");
  const [inviting, setInviting] = useState(false);
  const [flash, setFlash] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const list = await t2tListRelationships();
      setRelationships(list);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function handleInvite(e: React.FormEvent) {
    e.preventDefault();
    if (!inviteEmail.trim() || inviting) return;
    setInviting(true);
    setError(null);
    setFlash(null);
    try {
      await t2tInvite(inviteEmail.trim(), inviteReason.trim() || undefined);
      setFlash(`Invite sent to ${inviteEmail.trim()}`);
      setInviteEmail("");
      setInviteReason("");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setInviting(false);
    }
  }

  async function handleAccept(r: T2tRelationship) {
    setError(null);
    try {
      await t2tAccept(r.id);
      setFlash(`Accepted invite from ${r.other_name ?? r.other_email ?? "Travis"}`);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleRevoke(r: T2tRelationship) {
    setError(null);
    try {
      await t2tRevoke(r.id);
      setFlash("Relationship revoked");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  // Split into buckets for cleaner presentation.
  const active = relationships.filter((r) => r.status === "active");
  const incomingPending = relationships.filter(
    (r) => r.status === "pending" && currentUserId && r.to_user_id === currentUserId,
  );
  const outgoingPending = relationships.filter(
    (r) => r.status === "pending" && (!currentUserId || r.from_user_id === currentUserId),
  );

  return (
    <div className="flex flex-col gap-4">
      <p className="text-bone-3 text-[11px] leading-relaxed">
        Travis-to-Travis lets your Travis talk to someone else's Travis — ask a
        question, get a reply. Each side keeps a veto on what gets sent.
        You'll see incoming asks in your workspace attention strip.
      </p>

      {/* Invite form */}
      <form onSubmit={handleInvite} className="flex flex-col gap-2">
        <div className="flex gap-2">
          <input
            type="email"
            placeholder="their@email.com"
            value={inviteEmail}
            onChange={(e) => setInviteEmail(e.target.value)}
            disabled={inviting}
            className="flex-1 bg-white/[0.02] border rounded-md px-2 py-1.5 text-[13px] text-white placeholder:text-white/40 focus:outline-none focus:border-[#bd9eff]/40 disabled:opacity-50"
            style={{ borderColor: "rgba(255, 255, 255, 0.1)" }}
          />
          <motion.button
            type="submit"
            disabled={inviting || !inviteEmail.trim()}
            whileHover={{ scale: 1.02 }}
            whileTap={{ scale: 0.98 }}
            className="shrink-0 text-[11px] uppercase tracking-wider font-mono px-3 py-1.5 rounded-md transition-colors disabled:opacity-40"
            style={{
              background: "rgba(189, 158, 255, 0.12)",
              color: "rgb(189, 158, 255)",
              border: "1px solid rgba(189, 158, 255, 0.4)",
            }}
          >
            {inviting ? "sending…" : "invite"}
          </motion.button>
        </div>
        <input
          type="text"
          placeholder="Optional note ('for planning our trip', 'for the campaign')"
          value={inviteReason}
          onChange={(e) => setInviteReason(e.target.value)}
          disabled={inviting}
          className="bg-white/[0.02] border rounded-md px-2 py-1.5 text-[12px] text-white placeholder:text-white/40 focus:outline-none focus:border-[#bd9eff]/40 disabled:opacity-50"
          style={{ borderColor: "rgba(255, 255, 255, 0.08)" }}
        />
      </form>

      <AnimatePresence>
        {flash && (
          <motion.div
            key="flash"
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
            className="text-[11px] font-mono"
            style={{ color: "rgb(129, 199, 132)" }}
          >
            {flash}
          </motion.div>
        )}
        {error && (
          <motion.div
            key="error"
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
            className="text-[11px] font-mono"
            style={{ color: "rgba(255, 130, 130, 0.9)" }}
          >
            {error}
          </motion.div>
        )}
      </AnimatePresence>

      {/* Incoming pending — most urgent, top */}
      {incomingPending.length > 0 && (
        <RelationshipGroup
          title="Incoming invites"
          items={incomingPending}
          tone="incoming"
          onAccept={handleAccept}
          onRevoke={handleRevoke}
        />
      )}

      {/* Active relationships */}
      {active.length > 0 && (
        <RelationshipGroup
          title="Active"
          items={active}
          tone="active"
          onRevoke={handleRevoke}
        />
      )}

      {/* Outgoing pending */}
      {outgoingPending.length > 0 && (
        <RelationshipGroup
          title="Sent, waiting"
          items={outgoingPending}
          tone="outgoing"
          onRevoke={handleRevoke}
        />
      )}

      {!loading &&
        active.length === 0 &&
        incomingPending.length === 0 &&
        outgoingPending.length === 0 && (
          <div className="text-bone-3 text-[11px] font-mono opacity-60">
            No relationships yet. Invite someone above.
          </div>
        )}
    </div>
  );
}

// ─── Sub: relationship group list ────────────────────────────────

interface GroupProps {
  title: string;
  items: T2tRelationship[];
  tone: "incoming" | "active" | "outgoing";
  onAccept?: (r: T2tRelationship) => void;
  onRevoke?: (r: T2tRelationship) => void;
}

function RelationshipGroup({ title, items, tone, onAccept, onRevoke }: GroupProps) {
  const accent =
    tone === "incoming"
      ? "rgb(255, 179, 92)"
      : tone === "active"
        ? "rgb(129, 199, 132)"
        : "rgba(236, 236, 241, 0.6)";
  return (
    <div className="flex flex-col gap-1.5">
      <div
        className="text-[9px] uppercase tracking-[0.24em] font-mono"
        style={{ color: accent }}
      >
        {title}
      </div>
      {items.map((r) => (
        <div
          key={r.id}
          className="flex items-center justify-between gap-3 px-2.5 py-1.5 rounded-md border"
          style={{
            borderColor: `${accent.replace("rgb", "rgba").replace(")", ", 0.28)")}`,
            background: `${accent.replace("rgb", "rgba").replace(")", ", 0.04)")}`,
          }}
        >
          <div className="min-w-0 flex-1">
            <div className="text-[12.5px]" style={{ color: "rgba(236, 236, 241, 0.9)" }}>
              {r.other_name ?? r.other_email ?? "Unknown"}
            </div>
            {r.other_email && r.other_name && (
              <div
                className="text-[10px] font-mono opacity-60"
                style={{ color: "rgba(236, 236, 241, 0.7)" }}
              >
                {r.other_email}
              </div>
            )}
            {r.reason && (
              <div
                className="text-[10.5px] mt-0.5 italic opacity-70"
                style={{ color: "rgba(236, 236, 241, 0.7)" }}
              >
                {r.reason}
              </div>
            )}
          </div>
          <div className="flex items-center gap-1.5 shrink-0">
            {tone === "incoming" && onAccept && (
              <motion.button
                whileHover={{ scale: 1.04 }}
                whileTap={{ scale: 0.96 }}
                onClick={() => onAccept(r)}
                className="text-[10px] uppercase tracking-wider font-mono px-2 py-1 rounded"
                style={{
                  background: "rgba(129, 199, 132, 0.15)",
                  color: "rgb(129, 199, 132)",
                  border: "1px solid rgba(129, 199, 132, 0.4)",
                }}
              >
                accept
              </motion.button>
            )}
            {onRevoke && (
              <motion.button
                whileHover={{ scale: 1.04 }}
                whileTap={{ scale: 0.96 }}
                onClick={() => onRevoke(r)}
                className="text-[10px] uppercase tracking-wider font-mono px-2 py-1 rounded"
                style={{
                  background: "rgba(255, 255, 255, 0.03)",
                  color: "rgba(255, 180, 180, 0.85)",
                  border: "1px solid rgba(255, 100, 100, 0.25)",
                }}
              >
                revoke
              </motion.button>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}
