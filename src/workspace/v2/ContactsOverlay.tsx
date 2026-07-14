/**
 * ContactsOverlay — v0.28.45 discovery-first.
 *
 * Travis-to-Travis contacts + peer discovery. The default view is
 * discovery (AirDrop / Bluetooth style): who's nearby on the LAN,
 * plus your existing contacts. "Invite by email" is a secondary
 * action tucked behind a button — the primary interaction is
 * clicking on a Travis you can already see.
 *
 * Layout (top → bottom):
 *   1. Header
 *   2. NEARBY row — mDNS-discovered peers as tiles, polled every 2s.
 *      Empty state shows "Looking for Travises nearby…"
 *   3. YOUR CONTACTS row — accepted relationships + incoming pending
 *      (with accept) + outgoing pending (with revoke)
 *   4. Invite-by-email button. Click expands an inline form.
 *
 * Mounted from WorkspaceV2. Opens via the dock's Contacts icon or
 * ⌘⇧C.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import QRCode from "qrcode";
import { useAppStore } from "../../stores/app";
import {
  cloudStatus,
  discoveryPeers,
  discoveryStart,
  t2tAccept,
  t2tInvite,
  t2tListRelationships,
  t2tPairCreateToken,
  t2tPairRedeem,
  t2tRevoke,
  type DiscoveredPeer,
  type PairToken,
  type T2tRelationship,
} from "../../lib/cloud";

const DISCOVERY_POLL_MS = 2000;

export function ContactsOverlay() {
  const open = useAppStore((s) => s.contactsOverlayOpen);
  const setOpen = useAppStore((s) => s.setContactsOverlayOpen);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setOpen]);

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          key="contacts-overlay-backdrop"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.28, ease: [0.22, 1, 0.36, 1] }}
          className="fixed inset-0 z-40 flex items-center justify-center"
          style={{
            background: "rgba(0, 0, 0, 0.55)",
            backdropFilter: "blur(4px)",
          }}
          onClick={() => setOpen(false)}
        >
          <motion.div
            key="contacts-overlay-card"
            initial={{ opacity: 0, scale: 0.98, y: 8 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.98, y: 8 }}
            transition={{ duration: 0.32, ease: [0.22, 1, 0.36, 1] }}
            className="relative rounded-2xl overflow-hidden shadow-2xl"
            style={{
              width: "min(760px, 92vw)",
              height: "min(700px, 88vh)",
              background: "rgb(12, 12, 16)",
              border: "1px solid rgba(255, 255, 255, 0.10)",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              onClick={() => setOpen(false)}
              className="absolute top-3 right-3 z-10 w-8 h-8 rounded-full flex items-center justify-center transition-colors"
              style={{
                background: "rgba(255, 255, 255, 0.04)",
                border: "1px solid rgba(255, 255, 255, 0.08)",
                color: "rgba(236, 236, 241, 0.7)",
              }}
              title="Close (esc)"
            >
              ✕
            </button>
            <div className="h-full overflow-y-auto p-6">
              <ContactsBody open={open} />
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function ContactsBody({ open }: { open: boolean }) {
  const [currentUserId, setCurrentUserId] = useState<string | undefined>();
  const [peers, setPeers] = useState<DiscoveredPeer[]>([]);
  const [relationships, setRelationships] = useState<T2tRelationship[]>([]);
  const [flash, setFlash] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [inviteOpen, setInviteOpen] = useState(false);
  const [pendingInviteEmail, setPendingInviteEmail] = useState<string | null>(null);
  const [shareOpen, setShareOpen] = useState(false);
  const [redeemOpen, setRedeemOpen] = useState(false);
  const [prefillRedeemToken, setPrefillRedeemToken] = useState<string | null>(null);

  const refreshRelationships = useCallback(async () => {
    try {
      const list = await t2tListRelationships();
      setRelationships(list);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Kick off discovery daemon + poll peer list while the overlay's open.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;

    void discoveryStart().catch(() => {});
    void cloudStatus()
      .then((s) => {
        if (!cancelled && s.signedIn && s.user) setCurrentUserId(s.user.id);
      })
      .catch(() => {});
    void refreshRelationships();

    const poll = async () => {
      try {
        const list = await discoveryPeers();
        if (!cancelled) setPeers(list);
      } catch {
        /* discovery may not have initialized on the first tick */
      }
    };
    void poll();
    const t = window.setInterval(poll, DISCOVERY_POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(t);
    };
  }, [open, refreshRelationships]);

  // Auto-clear flashes so the UI settles.
  useEffect(() => {
    if (!flash) return;
    const t = window.setTimeout(() => setFlash(null), 3200);
    return () => window.clearTimeout(t);
  }, [flash]);

  // Bucket relationships.
  const active = relationships.filter((r) => r.status === "active");
  const incomingPending = relationships.filter(
    (r) => r.status === "pending" && currentUserId && r.to_user_id === currentUserId,
  );
  const outgoingPending = relationships.filter(
    (r) => r.status === "pending" && (!currentUserId || r.from_user_id === currentUserId),
  );

  // Set of emails already invited or paired so nearby chips don't
  // show an "invite" affordance for someone you already have.
  const knownEmails = new Set(
    relationships.map((r) => r.other_email?.toLowerCase()).filter(Boolean) as string[],
  );

  const invitePeer = async (peer: DiscoveredPeer) => {
    if (!peer.user_email) {
      // No email in the mDNS TXT record — fall back to opening the
      // invite form with a helpful message so the user can type it.
      setInviteOpen(true);
      setFlash(
        `${peer.display_name ?? "That Travis"} didn't broadcast an email — invite by address instead.`,
      );
      return;
    }
    try {
      await t2tInvite(peer.user_email, undefined);
      setFlash(`Invite sent to ${peer.display_name ?? peer.user_email}`);
      await refreshRelationships();
    } catch (e) {
      setError(String(e));
    }
  };

  const accept = async (r: T2tRelationship) => {
    try {
      await t2tAccept(r.id);
      setFlash(
        `Accepted invite from ${r.other_name ?? r.other_email ?? "that Travis"}`,
      );
      await refreshRelationships();
    } catch (e) {
      setError(String(e));
    }
  };

  const revoke = async (r: T2tRelationship) => {
    try {
      await t2tRevoke(r.id);
      setFlash("Contact removed");
      await refreshRelationships();
    } catch (e) {
      setError(String(e));
    }
  };

  const inviteByEmail = async (email: string, reason?: string) => {
    try {
      await t2tInvite(email.trim(), reason?.trim() || undefined);
      setFlash(`Invite sent to ${email.trim()}`);
      setPendingInviteEmail(null);
      setInviteOpen(false);
      await refreshRelationships();
    } catch (e) {
      setError(String(e));
    }
  };

  const redeemPairCode = useCallback(
    async (rawToken: string) => {
      const token = rawToken.trim().toUpperCase();
      if (!token) return;
      try {
        const result = await t2tPairRedeem(token);
        const label =
          result.other_user?.name ??
          result.other_user?.email ??
          "another Travis";
        setFlash(`Paired with ${label}`);
        setRedeemOpen(false);
        setPrefillRedeemToken(null);
        await refreshRelationships();
      } catch (e) {
        setError(String(e));
      }
    },
    [refreshRelationships],
  );

  // v0.28.46 — WorkspaceV2 listens for `travis://pair` at the window
  // level and stashes the token in the store (so cold-boot deep links
  // still land). Here we watch the store; when a token appears, show
  // the redeem modal + auto-redeem, then clear it.
  const pendingPairToken = useAppStore((s) => s.pendingPairToken);
  const setPendingPairToken = useAppStore((s) => s.setPendingPairToken);
  useEffect(() => {
    if (!pendingPairToken) return;
    setRedeemOpen(true);
    setPrefillRedeemToken(pendingPairToken);
    void redeemPairCode(pendingPairToken);
    setPendingPairToken(null);
  }, [pendingPairToken, redeemPairCode, setPendingPairToken]);

  return (
    <>
      <header className="mb-5">
        <div
          className="text-[10px] uppercase tracking-[0.22em] font-mono mb-1"
          style={{ color: "rgba(189, 158, 255, 0.85)" }}
        >
          // travis contacts
        </div>
        <h2
          className="text-[19px] font-medium leading-tight"
          style={{ color: "rgb(240, 240, 246)" }}
        >
          Your Travis network
        </h2>
        <p
          className="text-[12.5px] mt-1 leading-relaxed"
          style={{ color: "rgba(236, 236, 241, 0.68)" }}
        >
          Travises nearby show up here automatically. Tap one to send
          them an invite — once accepted, you can address them by name
          in any conversation.
        </p>
      </header>

      {/* NEARBY */}
      <SectionHeader label="Nearby" hint={peers.length > 0 ? `${peers.length} within reach` : undefined} />
      <div className="mt-2 min-h-[104px]">
        {peers.length === 0 ? (
          <EmptyNearby />
        ) : (
          <div className="grid grid-cols-2 sm:grid-cols-3 gap-2.5">
            {peers.map((p) => {
              const alreadyKnown =
                p.user_email != null && knownEmails.has(p.user_email.toLowerCase());
              return (
                <PeerCard
                  key={p.instance_name}
                  peer={p}
                  alreadyKnown={alreadyKnown}
                  onInvite={() => invitePeer(p)}
                />
              );
            })}
          </div>
        )}
      </div>

      {/* CONTACTS */}
      <div className="mt-6">
        <SectionHeader
          label="Your contacts"
          hint={active.length > 0 ? `${active.length} active` : undefined}
        />
        <div className="mt-2 flex flex-col gap-1.5">
          {incomingPending.map((r) => (
            <RelationshipRow
              key={r.id}
              relationship={r}
              variant="incoming"
              onAccept={() => accept(r)}
              onRevoke={() => revoke(r)}
            />
          ))}
          {active.map((r) => (
            <RelationshipRow
              key={r.id}
              relationship={r}
              variant="active"
              onRevoke={() => revoke(r)}
            />
          ))}
          {outgoingPending.map((r) => (
            <RelationshipRow
              key={r.id}
              relationship={r}
              variant="outgoing"
              onRevoke={() => revoke(r)}
            />
          ))}
          {active.length + incomingPending.length + outgoingPending.length === 0 && (
            <p
              className="text-[12px] leading-relaxed"
              style={{ color: "rgba(236, 236, 241, 0.55)" }}
            >
              No contacts yet. Tap a nearby Travis above, or invite one by
              email below.
            </p>
          )}
        </div>
      </div>

      {/* PAIR + INVITE ACTIONS */}
      <div className="mt-6 pt-4 border-t border-white/[0.06]">
        {!inviteOpen ? (
          <div className="flex flex-wrap gap-2">
            <SecondaryAction onClick={() => setShareOpen(true)} icon={<QrIcon />}>
              Share pair code
            </SecondaryAction>
            <SecondaryAction
              onClick={() => setRedeemOpen(true)}
              icon={<EnterIcon />}
            >
              Enter pair code
            </SecondaryAction>
            <SecondaryAction onClick={() => setInviteOpen(true)}>
              + Invite by email
            </SecondaryAction>
          </div>
        ) : (
          <InviteForm
            initialEmail={pendingInviteEmail ?? ""}
            onCancel={() => {
              setInviteOpen(false);
              setPendingInviteEmail(null);
            }}
            onSubmit={inviteByEmail}
          />
        )}
      </div>

      <SharePairModal open={shareOpen} onClose={() => setShareOpen(false)} />
      <RedeemPairModal
        open={redeemOpen}
        initialToken={prefillRedeemToken ?? ""}
        onCancel={() => {
          setRedeemOpen(false);
          setPrefillRedeemToken(null);
        }}
        onSubmit={redeemPairCode}
      />

      {/* Flashes */}
      <AnimatePresence>
        {(flash || error) && (
          <motion.div
            key="flash"
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
            className="mt-4 text-[11.5px] font-mono"
            style={{ color: error ? "rgb(255, 155, 155)" : "rgb(140, 230, 175)" }}
          >
            {error ?? flash}
          </motion.div>
        )}
      </AnimatePresence>
    </>
  );
}

// ─── Sub-components ────────────────────────────────────────────────

function SectionHeader({ label, hint }: { label: string; hint?: string }) {
  return (
    <div className="flex items-baseline justify-between">
      <div
        className="text-[10px] uppercase tracking-[0.22em] font-mono"
        style={{ color: "rgba(189, 158, 255, 0.75)" }}
      >
        {label}
      </div>
      {hint && (
        <div
          className="text-[10.5px] font-mono"
          style={{ color: "rgba(236, 236, 241, 0.42)" }}
        >
          {hint}
        </div>
      )}
    </div>
  );
}

function EmptyNearby() {
  return (
    <div
      className="rounded-xl px-4 py-5 flex items-center gap-3"
      style={{
        background: "rgba(255, 255, 255, 0.02)",
        border: "1px dashed rgba(255, 255, 255, 0.10)",
      }}
    >
      <div className="relative w-6 h-6 shrink-0">
        {[0, 0.3, 0.6].map((delay) => (
          <motion.span
            key={delay}
            className="absolute inset-0 rounded-full"
            animate={{ scale: [0.4, 1.6], opacity: [0.75, 0] }}
            transition={{ duration: 1.8, repeat: Infinity, delay, ease: "easeOut" }}
            style={{
              background: "rgba(189, 158, 255, 0.35)",
            }}
          />
        ))}
        <span
          className="absolute inset-1.5 rounded-full"
          style={{
            background: "rgba(189, 158, 255, 0.85)",
            boxShadow: "0 0 10px rgba(189, 158, 255, 0.7)",
          }}
        />
      </div>
      <div className="min-w-0 flex-1">
        <div
          className="text-[13px]"
          style={{ color: "rgba(240, 240, 246, 0.88)" }}
        >
          Looking for Travises nearby…
        </div>
        <div
          className="text-[11px] mt-0.5"
          style={{ color: "rgba(236, 236, 241, 0.55)" }}
        >
          They'll appear when they have Travis open on the same network.
        </div>
      </div>
    </div>
  );
}

function PeerCard({
  peer,
  alreadyKnown,
  onInvite,
}: {
  peer: DiscoveredPeer;
  alreadyKnown: boolean;
  onInvite: () => void;
}) {
  const name = peer.display_name ?? peer.instance_name;
  const initials = initialsFor(name);
  return (
    <motion.button
      onClick={alreadyKnown ? undefined : onInvite}
      disabled={alreadyKnown}
      whileHover={alreadyKnown ? {} : { y: -2 }}
      whileTap={alreadyKnown ? {} : { scale: 0.98 }}
      transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
      className="relative flex flex-col items-center gap-1.5 rounded-xl px-3 py-3.5 text-left transition-colors group"
      style={{
        background: "rgba(255, 255, 255, 0.02)",
        border: "1px solid rgba(189, 158, 255, 0.22)",
        cursor: alreadyKnown ? "default" : "pointer",
        opacity: alreadyKnown ? 0.65 : 1,
      }}
      title={
        alreadyKnown
          ? `You're already connected with ${name}`
          : `Invite ${name}`
      }
    >
      {/* Avatar disk */}
      <div
        className="relative w-11 h-11 rounded-full flex items-center justify-center"
        style={{
          background:
            "linear-gradient(140deg, rgba(189,158,255,0.42), rgba(140,105,235,0.35))",
          border: "1px solid rgba(189,158,255,0.55)",
          color: "rgba(250, 248, 255, 0.95)",
          fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
          fontSize: 12,
          fontWeight: 500,
          letterSpacing: "0.06em",
          boxShadow:
            "0 0 12px -2px rgba(189, 158, 255, 0.55), inset 0 0 6px rgba(255,255,255,0.15)",
        }}
      >
        {initials}
        {/* Pulse ring — signals live */}
        <motion.span
          className="absolute inset-0 rounded-full"
          animate={{ scale: [1, 1.35], opacity: [0.5, 0] }}
          transition={{ duration: 2, repeat: Infinity, ease: "easeOut" }}
          style={{ border: "1px solid rgba(189, 158, 255, 0.55)" }}
          aria-hidden
        />
      </div>
      <div
        className="text-[12.5px] font-medium leading-tight text-center truncate w-full"
        style={{ color: "rgba(240, 240, 246, 0.94)" }}
      >
        {name}
      </div>
      <div
        className="text-[10px] font-mono"
        style={{
          color: alreadyKnown
            ? "rgba(140, 230, 175, 0.85)"
            : "rgba(189, 158, 255, 0.85)",
        }}
      >
        {alreadyKnown ? "connected" : "invite"}
      </div>
    </motion.button>
  );
}

function RelationshipRow({
  relationship: r,
  variant,
  onAccept,
  onRevoke,
}: {
  relationship: T2tRelationship;
  variant: "incoming" | "outgoing" | "active";
  onAccept?: () => void;
  onRevoke: () => void;
}) {
  const name = r.other_name ?? r.other_email ?? "unknown";
  const detail =
    variant === "incoming"
      ? "invited you"
      : variant === "outgoing"
      ? "invite pending"
      : r.other_email ?? "connected";

  const accentColor =
    variant === "active"
      ? "rgba(140, 230, 175, 0.85)"
      : variant === "incoming"
      ? "rgba(255, 210, 130, 0.9)"
      : "rgba(189, 158, 255, 0.85)";

  return (
    <div
      className="flex items-center gap-3 rounded-lg px-3 py-2"
      style={{
        background: "rgba(255, 255, 255, 0.02)",
        border: "1px solid rgba(255, 255, 255, 0.06)",
      }}
    >
      <div
        className="w-8 h-8 rounded-full flex items-center justify-center shrink-0"
        style={{
          background: "rgba(189, 158, 255, 0.18)",
          border: "1px solid rgba(189, 158, 255, 0.35)",
          color: "rgba(250, 248, 255, 0.9)",
          fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
          fontSize: 10.5,
        }}
      >
        {initialsFor(name)}
      </div>
      <div className="min-w-0 flex-1">
        <div
          className="text-[13px] font-medium truncate"
          style={{ color: "rgba(240, 240, 246, 0.94)" }}
        >
          {name}
        </div>
        <div
          className="text-[10.5px] font-mono"
          style={{ color: accentColor }}
        >
          {detail}
        </div>
      </div>
      {variant === "incoming" && onAccept && (
        <button
          onClick={onAccept}
          className="text-[10.5px] uppercase tracking-[0.16em] font-mono px-2.5 py-1 rounded-md transition-colors"
          style={{
            background: "rgba(140, 230, 175, 0.14)",
            color: "rgb(140, 230, 175)",
            border: "1px solid rgba(140, 230, 175, 0.45)",
          }}
        >
          accept
        </button>
      )}
      <button
        onClick={onRevoke}
        className="text-[10.5px] uppercase tracking-[0.16em] font-mono px-2.5 py-1 rounded-md transition-colors"
        style={{
          background: "rgba(255, 255, 255, 0.04)",
          color: "rgba(236, 236, 241, 0.65)",
          border: "1px solid rgba(255, 255, 255, 0.10)",
        }}
      >
        {variant === "outgoing" || variant === "incoming" ? "cancel" : "remove"}
      </button>
    </div>
  );
}

function InviteForm({
  initialEmail,
  onCancel,
  onSubmit,
}: {
  initialEmail: string;
  onCancel: () => void;
  onSubmit: (email: string, reason?: string) => void;
}) {
  const [email, setEmail] = useState(initialEmail);
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!email.trim() || busy) return;
    setBusy(true);
    try {
      await onSubmit(email, reason);
    } finally {
      setBusy(false);
    }
  };

  return (
    <motion.form
      initial={{ opacity: 0, y: -4 }}
      animate={{ opacity: 1, y: 0 }}
      onSubmit={submit}
      className="flex flex-col gap-2.5"
    >
      <div
        className="text-[10px] uppercase tracking-[0.22em] font-mono"
        style={{ color: "rgba(189, 158, 255, 0.75)" }}
      >
        // invite by email
      </div>
      <div className="flex gap-2">
        <input
          type="email"
          placeholder="their@email.com"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          disabled={busy}
          autoFocus
          className="flex-1 bg-white/[0.02] border rounded-md px-3 py-1.5 text-[13px] text-white placeholder:text-white/40 focus:outline-none focus:border-[#bd9eff]/40 disabled:opacity-50"
          style={{ borderColor: "rgba(255, 255, 255, 0.1)" }}
        />
        <button
          type="submit"
          disabled={busy || !email.trim()}
          className="shrink-0 text-[11px] uppercase tracking-wider font-mono px-3 py-1.5 rounded-md transition-colors disabled:opacity-40"
          style={{
            background: "rgba(189, 158, 255, 0.14)",
            color: "rgb(189, 158, 255)",
            border: "1px solid rgba(189, 158, 255, 0.45)",
          }}
        >
          {busy ? "sending…" : "send"}
        </button>
        <button
          type="button"
          onClick={onCancel}
          disabled={busy}
          className="shrink-0 text-[11px] uppercase tracking-wider font-mono px-3 py-1.5 rounded-md transition-colors"
          style={{
            background: "rgba(255, 255, 255, 0.04)",
            color: "rgba(236, 236, 241, 0.65)",
            border: "1px solid rgba(255, 255, 255, 0.10)",
          }}
        >
          cancel
        </button>
      </div>
      <input
        type="text"
        placeholder="Optional note ('for planning our trip', 'for the campaign')"
        value={reason}
        onChange={(e) => setReason(e.target.value)}
        disabled={busy}
        className="bg-white/[0.02] border rounded-md px-3 py-1.5 text-[12px] text-white placeholder:text-white/40 focus:outline-none focus:border-[#bd9eff]/40 disabled:opacity-50"
        style={{ borderColor: "rgba(255, 255, 255, 0.08)" }}
      />
    </motion.form>
  );
}

function initialsFor(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "??";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

// ─── Pair action controls ──────────────────────────────────────────

function SecondaryAction({
  children,
  onClick,
  icon,
}: {
  children: React.ReactNode;
  onClick: () => void;
  icon?: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className="text-[11.5px] uppercase tracking-[0.18em] font-mono px-3 py-2 rounded-lg transition-colors inline-flex items-center gap-1.5"
      style={{
        background: "rgba(189, 158, 255, 0.10)",
        color: "rgb(189, 158, 255)",
        border: "1px solid rgba(189, 158, 255, 0.35)",
      }}
    >
      {icon}
      {children}
    </button>
  );
}

function QrIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <rect x="3" y="3" width="7" height="7" />
      <rect x="14" y="3" width="7" height="7" />
      <rect x="3" y="14" width="7" height="7" />
      <path d="M14 14h3v3h-3zM19 14h2M14 19h2M19 21h2" />
    </svg>
  );
}

function EnterIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M4 12h12M12 6l6 6-6 6" />
      <path d="M20 4v16" />
    </svg>
  );
}

// ─── Share pair modal ──────────────────────────────────────────────

function SharePairModal({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const [token, setToken] = useState<PairToken | null>(null);
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [copyFlash, setCopyFlash] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const issue = useCallback(async () => {
    setBusy(true);
    setErr(null);
    try {
      const t = await t2tPairCreateToken();
      setToken(t);
      // Render the QR from the deep link so scanning opens Travis
      // automatically instead of just displaying the code text.
      const png = await QRCode.toDataURL(t.deep_link, {
        errorCorrectionLevel: "M",
        margin: 1,
        width: 260,
        color: {
          dark: "#EDE7FF",
          light: "#00000000",
        },
      });
      setQrDataUrl(png);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    if (open && !token) void issue();
    if (!open) {
      // Reset on close so a fresh token issues next time.
      setToken(null);
      setQrDataUrl(null);
      setErr(null);
    }
  }, [open, token, issue]);

  const copyCode = () => {
    if (!token) return;
    void navigator.clipboard.writeText(token.token);
    setCopyFlash(true);
    window.setTimeout(() => setCopyFlash(false), 1500);
  };

  const copyLink = () => {
    if (!token) return;
    void navigator.clipboard.writeText(token.deep_link);
    setCopyFlash(true);
    window.setTimeout(() => setCopyFlash(false), 1500);
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          key="share-backdrop"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.24 }}
          className="fixed inset-0 z-50 flex items-center justify-center px-4"
          style={{ background: "rgba(0, 0, 0, 0.6)", backdropFilter: "blur(4px)" }}
          onClick={onClose}
        >
          <motion.div
            initial={{ opacity: 0, y: 12, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 8, scale: 0.98 }}
            transition={{ duration: 0.28, ease: [0.22, 1, 0.36, 1] }}
            className="relative rounded-2xl overflow-hidden"
            style={{
              width: "min(420px, 92vw)",
              background: "rgb(14, 12, 20)",
              border: "1px solid rgba(189, 158, 255, 0.32)",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              onClick={onClose}
              className="absolute top-3 right-3 z-10 w-7 h-7 rounded-full flex items-center justify-center"
              style={{
                background: "rgba(255, 255, 255, 0.04)",
                border: "1px solid rgba(255, 255, 255, 0.08)",
                color: "rgba(236, 236, 241, 0.7)",
              }}
              title="Close"
            >
              ✕
            </button>
            <div className="p-6 flex flex-col items-center gap-4">
              <div
                className="text-[10px] uppercase tracking-[0.22em] font-mono"
                style={{ color: "rgba(189, 158, 255, 0.85)" }}
              >
                // pair code
              </div>
              <div
                className="rounded-xl p-4"
                style={{
                  background: "rgba(255, 255, 255, 0.03)",
                  border: "1px solid rgba(189, 158, 255, 0.22)",
                  width: 260,
                  height: 260,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                }}
              >
                {qrDataUrl ? (
                  <img
                    src={qrDataUrl}
                    alt="Pair code QR"
                    width={228}
                    height={228}
                    style={{ imageRendering: "pixelated" }}
                  />
                ) : (
                  <div
                    className="text-[11px] font-mono"
                    style={{ color: "rgba(236, 236, 241, 0.6)" }}
                  >
                    {err ? err : busy ? "issuing…" : ""}
                  </div>
                )}
              </div>
              {token && (
                <>
                  <button
                    onClick={copyCode}
                    className="text-[22px] font-mono px-4 py-2 rounded-lg tracking-[0.24em] transition-colors"
                    style={{
                      background: "rgba(189, 158, 255, 0.10)",
                      color: "rgb(240, 240, 246)",
                      border: "1px solid rgba(189, 158, 255, 0.35)",
                      letterSpacing: "0.24em",
                    }}
                    title="Copy the code"
                  >
                    {token.token}
                  </button>
                  <p
                    className="text-[11.5px] text-center leading-relaxed max-w-xs"
                    style={{ color: "rgba(236, 236, 241, 0.62)" }}
                  >
                    They can scan the QR to open Travis and pair, or type the
                    code above into their <strong>Enter pair code</strong> flow.
                    Expires in 24 hours.
                  </p>
                  <button
                    onClick={copyLink}
                    className="text-[11px] uppercase tracking-[0.18em] font-mono px-3 py-1.5 rounded-md"
                    style={{
                      background: "rgba(255, 255, 255, 0.04)",
                      color: "rgba(236, 236, 241, 0.75)",
                      border: "1px solid rgba(255, 255, 255, 0.1)",
                    }}
                  >
                    {copyFlash ? "copied ✓" : "copy deep link"}
                  </button>
                </>
              )}
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

// ─── Redeem pair modal ─────────────────────────────────────────────

function RedeemPairModal({
  open,
  initialToken,
  onCancel,
  onSubmit,
}: {
  open: boolean;
  initialToken: string;
  onCancel: () => void;
  onSubmit: (token: string) => Promise<void>;
}) {
  const [value, setValue] = useState(initialToken);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (open) {
      setValue(initialToken);
      // Focus so a paste + Enter works immediately.
      window.setTimeout(() => inputRef.current?.focus(), 60);
    }
  }, [open, initialToken]);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!value.trim() || busy) return;
    setBusy(true);
    try {
      await onSubmit(value);
    } finally {
      setBusy(false);
    }
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          key="redeem-backdrop"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.24 }}
          className="fixed inset-0 z-50 flex items-center justify-center px-4"
          style={{ background: "rgba(0, 0, 0, 0.6)", backdropFilter: "blur(4px)" }}
          onClick={onCancel}
        >
          <motion.form
            onSubmit={submit}
            initial={{ opacity: 0, y: 12, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 8, scale: 0.98 }}
            transition={{ duration: 0.28, ease: [0.22, 1, 0.36, 1] }}
            className="relative rounded-2xl overflow-hidden p-6 flex flex-col gap-4"
            style={{
              width: "min(400px, 92vw)",
              background: "rgb(14, 12, 20)",
              border: "1px solid rgba(189, 158, 255, 0.32)",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <div>
              <div
                className="text-[10px] uppercase tracking-[0.22em] font-mono mb-1"
                style={{ color: "rgba(189, 158, 255, 0.85)" }}
              >
                // enter pair code
              </div>
              <p
                className="text-[12.5px] leading-relaxed"
                style={{ color: "rgba(236, 236, 241, 0.72)" }}
              >
                Ask the other person to share their pair code from{" "}
                <em>Contacts → Share pair code</em>. Then type the 8-character
                code below.
              </p>
            </div>
            <input
              ref={inputRef}
              value={value}
              onChange={(e) => setValue(e.target.value.toUpperCase())}
              disabled={busy}
              maxLength={12}
              spellCheck={false}
              autoCapitalize="characters"
              autoComplete="off"
              placeholder="ABCD2345"
              className="w-full bg-white/[0.02] border rounded-md px-3 py-2 text-[18px] font-mono text-center tracking-[0.32em] text-white placeholder:text-white/25 focus:outline-none focus:border-[#bd9eff]/50 disabled:opacity-50"
              style={{ borderColor: "rgba(255, 255, 255, 0.12)" }}
            />
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={onCancel}
                disabled={busy}
                className="text-[11.5px] uppercase tracking-[0.16em] font-mono px-3 py-1.5 rounded-md"
                style={{
                  background: "rgba(255, 255, 255, 0.04)",
                  color: "rgba(236, 236, 241, 0.75)",
                  border: "1px solid rgba(255, 255, 255, 0.1)",
                }}
              >
                cancel
              </button>
              <button
                type="submit"
                disabled={busy || !value.trim()}
                className="text-[11.5px] uppercase tracking-[0.16em] font-mono px-4 py-1.5 rounded-md disabled:opacity-40"
                style={{
                  background: "rgba(189, 158, 255, 0.16)",
                  color: "rgb(189, 158, 255)",
                  border: "1px solid rgba(189, 158, 255, 0.45)",
                }}
              >
                {busy ? "pairing…" : "pair"}
              </button>
            </div>
          </motion.form>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
