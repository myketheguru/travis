/**
 * ContactsOverlay — v0.28.49 radar redesign.
 *
 * Four tabs so the network overview reads as one coherent surface
 * instead of a menu of buttons:
 *
 *   Discover  — animated radar screen with nearby Travises as blips
 *               (mDNS today, BLE plugs in via v0.28.50). Click a blip
 *               to pair. Below the radar is a plain list mirror so
 *               peers stay accessible without hunting the dots.
 *   Circles   — your named groups. Inline "create" and "join" cards
 *               live at the top of the tab, so no separate modal
 *               chrome.
 *   Contacts  — accepted + pending relationships, with the invite
 *               form inline at the top.
 *   Pair      — share pair code (QR + 8-char) on one side, redeem on
 *               the other. Deep link travis://pair?tok=... auto-
 *               switches to this tab and prefills the code.
 *
 * Peer model is unified across discovery sources — Peer.source tells
 * the UI whether the tile came from mDNS or BLE (v0.28.50). Each row
 * also knows its relationship status so a nearby Travis you've
 * already paired with is styled + labeled differently from a stranger.
 *
 * Secure file transfer will attach a "Send file" affordance to each
 * peer/contact row in v0.28.50 once the BLE + T2T transports land.
 * The scaffold (`ble_send_file`) already returns a friendly
 * "coming next release" message — the UI toasts it cleanly.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import QRCode from "qrcode";
import { useAppStore } from "../../stores/app";
import {
  bleScanPeers,
  bleSendFile,
  circlesContacts,
  circlesCreate,
  circlesDelete,
  circlesJoin,
  circlesLeave,
  circlesList,
  circlesMembers,
  cloudStatus,
  discoveryPeers,
  discoveryStart,
  t2tAccept,
  t2tInvite,
  t2tListRelationships,
  t2tPairCreateToken,
  t2tPairRedeem,
  t2tRevoke,
  type BlePeer,
  type Circle,
  type CircleContact,
  type CircleMember,
  type DiscoveredPeer,
  type PairToken,
  type T2tRelationship,
} from "../../lib/cloud";

const DISCOVERY_POLL_MS = 2000;

type Tab = "discover" | "circles" | "contacts" | "pair";

/// Unified peer model across discovery sources. Every tile the UI
/// renders passes through this shape so the radar + list share one
/// data feed.
type PeerSource = "mdns" | "ble";
interface UnifiedPeer {
  id: string;             // stable per session
  source: PeerSource;
  displayName: string;
  userEmail: string | null;
  userId: string | null;
  rssi: number | null;    // BLE only, dBm
  /// Relationship state derived by matching userEmail against
  /// t2t_list_relationships. Null when we haven't matched.
  relationship: "active" | "pending" | null;
}

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
              width: "min(820px, 94vw)",
              height: "min(760px, 90vh)",
              background:
                "radial-gradient(120% 90% at 20% 0%, rgba(140,105,235,0.20), rgba(12,12,20,0.98) 55%), rgb(12, 12, 16)",
              border: "1px solid rgba(189, 158, 255, 0.28)",
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
            <ContactsBody open={open} />
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function ContactsBody({ open }: { open: boolean }) {
  const [tab, setTab] = useState<Tab>("discover");
  const [currentUserId, setCurrentUserId] = useState<string | undefined>();
  const [mdnsPeers, setMdnsPeers] = useState<DiscoveredPeer[]>([]);
  const [ble, setBle] = useState<BlePeer[]>([]);
  const [relationships, setRelationships] = useState<T2tRelationship[]>([]);
  const [circles, setCircles] = useState<Circle[]>([]);
  const [circleContacts, setCircleContacts] = useState<CircleContact[]>([]);
  const [flash, setFlash] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Pair tab pre-fill from a deep-link redemption.
  const pendingPairToken = useAppStore((s) => s.pendingPairToken);
  const setPendingPairToken = useAppStore((s) => s.setPendingPairToken);

  const refreshAll = useCallback(async () => {
    try {
      const [rel, cs, cc] = await Promise.all([
        t2tListRelationships(),
        circlesList().catch(() => [] as Circle[]),
        circlesContacts().catch(() => [] as CircleContact[]),
      ]);
      setRelationships(rel);
      setCircles(cs);
      setCircleContacts(cc);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Discovery poll (mDNS + BLE placeholder).
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    void discoveryStart().catch(() => {});
    void cloudStatus()
      .then((s) => {
        if (!cancelled && s.signedIn && s.user) setCurrentUserId(s.user.id);
      })
      .catch(() => {});
    void refreshAll();

    const tick = async () => {
      try {
        const [m, b] = await Promise.all([
          discoveryPeers().catch(() => [] as DiscoveredPeer[]),
          bleScanPeers().catch(() => [] as BlePeer[]),
        ]);
        if (!cancelled) {
          setMdnsPeers(m);
          setBle(b);
        }
      } catch {
        /* transient, no-op */
      }
    };
    void tick();
    const id = window.setInterval(tick, DISCOVERY_POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [open, refreshAll]);

  // Deep-link into Pair tab.
  useEffect(() => {
    if (open && pendingPairToken) setTab("pair");
  }, [open, pendingPairToken]);

  // Auto-clear flashes.
  useEffect(() => {
    if (!flash) return;
    const t = window.setTimeout(() => setFlash(null), 3000);
    return () => window.clearTimeout(t);
  }, [flash]);
  useEffect(() => {
    if (!error) return;
    const t = window.setTimeout(() => setError(null), 4500);
    return () => window.clearTimeout(t);
  }, [error]);

  // Bucket + dedup peers.
  const knownEmails = useMemo(
    () =>
      new Set(
        relationships
          .map((r) => r.other_email?.toLowerCase())
          .filter(Boolean) as string[],
      ),
    [relationships],
  );

  const activeRelEmails = useMemo(
    () =>
      new Set(
        relationships
          .filter((r) => r.status === "active")
          .map((r) => r.other_email?.toLowerCase())
          .filter(Boolean) as string[],
      ),
    [relationships],
  );

  const unifiedPeers = useMemo<UnifiedPeer[]>(() => {
    const list: UnifiedPeer[] = [];
    const seen = new Set<string>();
    for (const m of mdnsPeers) {
      const key = (m.user_email ?? m.instance_name).toLowerCase();
      if (seen.has(key)) continue;
      seen.add(key);
      const email = m.user_email ?? null;
      const emailKey = email?.toLowerCase();
      list.push({
        id: `mdns:${m.instance_name}`,
        source: "mdns",
        displayName: m.display_name ?? m.instance_name,
        userEmail: email,
        userId: m.user_id ?? null,
        rssi: null,
        relationship: emailKey && activeRelEmails.has(emailKey)
          ? "active"
          : emailKey && knownEmails.has(emailKey)
          ? "pending"
          : null,
      });
    }
    for (const b of ble) {
      const key = (b.user_id ?? b.instance_id).toLowerCase();
      if (seen.has(key)) continue;
      seen.add(key);
      list.push({
        id: `ble:${b.instance_id}`,
        source: "ble",
        displayName: b.display_name ?? b.instance_id,
        userEmail: null,
        userId: b.user_id ?? null,
        rssi: b.rssi ?? null,
        relationship: null,
      });
    }
    return list;
  }, [mdnsPeers, ble, knownEmails, activeRelEmails]);

  // Relationship buckets.
  const activeRels = relationships.filter((r) => r.status === "active");
  const incomingPending = relationships.filter(
    (r) => r.status === "pending" && currentUserId && r.to_user_id === currentUserId,
  );
  const outgoingPending = relationships.filter(
    (r) => r.status === "pending" && (!currentUserId || r.from_user_id === currentUserId),
  );

  // ─── Actions ─────────────────────────────────────────────────────

  const invitePeer = async (peer: UnifiedPeer) => {
    if (!peer.userEmail) {
      setTab("pair");
      setFlash(
        `${peer.displayName} didn't broadcast an email. Share a pair code instead.`,
      );
      return;
    }
    try {
      await t2tInvite(peer.userEmail);
      setFlash(`Invite sent to ${peer.displayName}`);
      await refreshAll();
    } catch (e) {
      setError(String(e));
    }
  };
  const inviteByEmail = async (email: string, reason?: string) => {
    try {
      await t2tInvite(email.trim(), reason?.trim() || undefined);
      setFlash(`Invite sent to ${email.trim()}`);
      await refreshAll();
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
      await refreshAll();
    } catch (e) {
      setError(String(e));
    }
  };
  const revoke = async (r: T2tRelationship) => {
    try {
      await t2tRevoke(r.id);
      setFlash("Contact removed");
      await refreshAll();
    } catch (e) {
      setError(String(e));
    }
  };
  const createCircle = async (name: string, description?: string) => {
    try {
      const c = await circlesCreate(name, description);
      setFlash(`Created "${c.name}" · code ${c.join_code}`);
      await refreshAll();
    } catch (e) {
      setError(String(e));
    }
  };
  const joinCircleByCode = async (code: string) => {
    try {
      const r = await circlesJoin(code);
      setFlash(r.already_member ? `Already in "${r.name}"` : `Joined "${r.name}"`);
      await refreshAll();
    } catch (e) {
      setError(String(e));
    }
  };
  const leaveCircle = async (c: Circle) => {
    try {
      await circlesLeave(c.id);
      setFlash(`Left "${c.name}"`);
      await refreshAll();
    } catch (e) {
      setError(String(e));
    }
  };
  const deleteCircle = async (c: Circle) => {
    try {
      await circlesDelete(c.id);
      setFlash(`Deleted "${c.name}"`);
      await refreshAll();
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
          result.other_user?.name ?? result.other_user?.email ?? "another Travis";
        setFlash(`Paired with ${label}`);
        setPendingPairToken(null);
        await refreshAll();
      } catch (e) {
        setError(String(e));
      }
    },
    [refreshAll, setPendingPairToken],
  );
  const sendFile = async (identifier: string) => {
    try {
      const msg = await bleSendFile(identifier, "");
      setFlash(msg);
    } catch (e) {
      // The v0.28.49 scaffold returns a friendly error. Treat as flash.
      setFlash(String(e).replace(/^Error:\s*/, ""));
    }
  };

  const nearbyCount = unifiedPeers.length;
  const totalContacts =
    activeRels.length + incomingPending.length + outgoingPending.length;

  return (
    <div className="h-full flex flex-col">
      <Header />
      <TabBar
        tab={tab}
        onSelect={setTab}
        counts={{
          discover: nearbyCount,
          circles: circles.length,
          contacts: totalContacts,
          pair: 0,
        }}
      />
      <div className="flex-1 min-h-0 overflow-y-auto px-6 pb-4">
        <AnimatePresence mode="wait">
          <motion.div
            key={tab}
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -6 }}
            transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
          >
            {tab === "discover" && (
              <DiscoverPane
                peers={unifiedPeers}
                onPair={invitePeer}
                onSendFile={(p) => sendFile(p.id)}
              />
            )}
            {tab === "circles" && (
              <CirclesPane
                circles={circles}
                onCreate={createCircle}
                onJoin={joinCircleByCode}
                onLeave={leaveCircle}
                onDelete={deleteCircle}
                onFlash={setFlash}
                onError={setError}
                contacts={circleContacts}
              />
            )}
            {tab === "contacts" && (
              <ContactsPane
                incoming={incomingPending}
                active={activeRels}
                outgoing={outgoingPending}
                onAccept={accept}
                onRevoke={revoke}
                onInvite={inviteByEmail}
                onSendFile={(r) => sendFile(r.other_email ?? r.other_name ?? r.id)}
              />
            )}
            {tab === "pair" && (
              <PairPane
                pendingToken={pendingPairToken ?? ""}
                onRedeem={redeemPairCode}
              />
            )}
          </motion.div>
        </AnimatePresence>
      </div>
      {/* Flash strip */}
      <AnimatePresence>
        {(flash || error) && (
          <motion.div
            key="flash-strip"
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 8 }}
            transition={{ duration: 0.22 }}
            className="mx-6 mb-4 rounded-lg px-3 py-2 text-[12px] font-mono"
            style={{
              background: error
                ? "rgba(255, 130, 130, 0.10)"
                : "rgba(140, 230, 175, 0.10)",
              border: `1px solid ${
                error ? "rgba(255, 130, 130, 0.35)" : "rgba(140, 230, 175, 0.35)"
              }`,
              color: error ? "rgb(255, 165, 165)" : "rgb(160, 235, 190)",
            }}
          >
            {error ?? flash}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

// ─── Chrome ────────────────────────────────────────────────────────

function Header() {
  return (
    <div className="px-6 pt-6 pb-4 shrink-0">
      <div
        className="text-[10px] uppercase tracking-[0.22em] font-mono mb-1"
        style={{ color: "rgba(189, 158, 255, 0.85)" }}
      >
        // travis contacts
      </div>
      <h2
        className="text-[20px] font-medium leading-tight"
        style={{ color: "rgb(240, 240, 246)" }}
      >
        Your Travis network
      </h2>
    </div>
  );
}

function TabBar({
  tab,
  onSelect,
  counts,
}: {
  tab: Tab;
  onSelect: (t: Tab) => void;
  counts: Record<Tab, number>;
}) {
  const tabs: { id: Tab; label: string; icon: React.ReactNode }[] = [
    { id: "discover", label: "Discover", icon: <RadarIcon /> },
    { id: "circles", label: "Circles", icon: <CircleIcon /> },
    { id: "contacts", label: "Contacts", icon: <PeopleIcon /> },
    { id: "pair", label: "Pair", icon: <QrIcon /> },
  ];
  return (
    <div className="px-6 shrink-0">
      <div
        className="flex items-center gap-1 p-1 rounded-xl"
        style={{
          background: "rgba(255, 255, 255, 0.03)",
          border: "1px solid rgba(255, 255, 255, 0.06)",
        }}
      >
        {tabs.map((t) => {
          const active = tab === t.id;
          return (
            <button
              key={t.id}
              onClick={() => onSelect(t.id)}
              className="relative flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg transition-colors text-[12.5px]"
              style={{
                color: active
                  ? "rgb(240, 240, 246)"
                  : "rgba(236, 236, 241, 0.62)",
              }}
            >
              {active && (
                <motion.div
                  layoutId="tab-pill"
                  className="absolute inset-0 rounded-lg"
                  transition={{ type: "spring", stiffness: 340, damping: 30 }}
                  style={{
                    background:
                      "linear-gradient(180deg, rgba(189,158,255,0.22), rgba(140,105,235,0.16))",
                    border: "1px solid rgba(189, 158, 255, 0.42)",
                    boxShadow: "0 0 18px -6px rgba(189, 158, 255, 0.55)",
                  }}
                />
              )}
              <span className="relative z-10 inline-flex items-center gap-1.5">
                {t.icon}
                <span>{t.label}</span>
                {counts[t.id] > 0 && (
                  <span
                    className="text-[10px] font-mono px-1 rounded"
                    style={{
                      color: active
                        ? "rgba(240, 240, 246, 0.85)"
                        : "rgba(189, 158, 255, 0.75)",
                    }}
                  >
                    {counts[t.id]}
                  </span>
                )}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

// ─── Discover tab ──────────────────────────────────────────────────

function DiscoverPane({
  peers,
  onPair,
  onSendFile,
}: {
  peers: UnifiedPeer[];
  onPair: (p: UnifiedPeer) => void;
  onSendFile: (p: UnifiedPeer) => void;
}) {
  const [hovered, setHovered] = useState<string | null>(null);
  return (
    <div className="mt-4">
      <RadarScene
        peers={peers}
        hovered={hovered}
        setHovered={setHovered}
        onPeerClick={onPair}
      />
      <div
        className="mt-4 text-[11px] font-mono flex items-center justify-between"
        style={{ color: "rgba(236, 236, 241, 0.55)" }}
      >
        <span>
          {peers.length === 0
            ? "scanning… nothing nearby yet"
            : `${peers.length} nearby${peers.some((p) => p.source === "ble") ? " · ble + mdns" : " · mdns"}`}
        </span>
        <span
          className="text-[10px] uppercase tracking-[0.18em]"
          style={{ color: "rgba(189, 158, 255, 0.65)" }}
        >
          live · updates every 2s
        </span>
      </div>
      <div className="mt-3 flex flex-col gap-1.5">
        {peers.map((p) => (
          <PeerListRow
            key={p.id}
            peer={p}
            highlighted={hovered === p.id}
            onHover={setHovered}
            onPair={() => onPair(p)}
            onSendFile={() => onSendFile(p)}
          />
        ))}
        {peers.length === 0 && (
          <div
            className="rounded-xl px-4 py-6 text-center text-[12.5px]"
            style={{
              background: "rgba(255, 255, 255, 0.02)",
              border: "1px dashed rgba(255, 255, 255, 0.10)",
              color: "rgba(236, 236, 241, 0.62)",
            }}
          >
            Nothing nearby yet. Others need Travis open on the same
            network — or hop over to the <strong>Pair</strong> tab and
            share a code.
          </div>
        )}
      </div>
    </div>
  );
}

/// The animated radar screen. Concentric brand-purple rings pulse
/// outward, a rotating sweep line does 360° every 3.2s, and each
/// discovered peer sits at a hashed angle at a radius derived from
/// signal strength (BLE) or a stable id hash (mDNS). Clicking a blip
/// invokes onPeerClick.
function RadarScene({
  peers,
  hovered,
  setHovered,
  onPeerClick,
}: {
  peers: UnifiedPeer[];
  hovered: string | null;
  setHovered: (id: string | null) => void;
  onPeerClick: (p: UnifiedPeer) => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState<{ w: number; h: number }>({ w: 720, h: 260 });

  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const rect = el.getBoundingClientRect();
      setSize({ w: rect.width, h: 260 });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Compute stable peer positions from their id hash.
  const positioned = useMemo(() => {
    return peers.map((p) => {
      const h = hashString(p.id);
      // Angle 0 = top of the screen so the sweep starts visually
      // where a real radar would. Angle wraps 0..2π.
      const angle = (h % 360) * (Math.PI / 180);
      // Radius as fraction of max. BLE uses rssi (closer to 0 → nearer).
      // mDNS uses hash so peers spread out but stay stable.
      let radiusFrac: number;
      if (p.source === "ble" && p.rssi != null) {
        // rssi ranges roughly -30 (near) to -95 (edge)
        const clamped = Math.max(-95, Math.min(-30, p.rssi));
        radiusFrac = (clamped + 30) / -65; // 0..1, 0 = near
        radiusFrac = 0.15 + radiusFrac * 0.75;
      } else {
        radiusFrac = 0.28 + ((h % 60) / 60) * 0.55;
      }
      return { peer: p, angle, radiusFrac };
    });
  }, [peers]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = Math.min(window.devicePixelRatio, 2);
    canvas.width = Math.floor(size.w * dpr);
    canvas.height = Math.floor(size.h * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    let raf = 0;
    const start = performance.now();

    const draw = () => {
      const now = performance.now();
      const t = (now - start) / 1000;
      const cx = size.w / 2;
      const cy = size.h / 2;
      const maxR = Math.min(size.w, size.h * 2) * 0.45;

      ctx.clearRect(0, 0, size.w, size.h);

      // Backdrop gradient
      const bg = ctx.createRadialGradient(cx, cy, 0, cx, cy, maxR * 1.4);
      bg.addColorStop(0, "rgba(140, 105, 235, 0.16)");
      bg.addColorStop(0.55, "rgba(24, 20, 44, 0.10)");
      bg.addColorStop(1, "rgba(0, 0, 0, 0)");
      ctx.fillStyle = bg;
      ctx.beginPath();
      ctx.arc(cx, cy, maxR * 1.4, 0, Math.PI * 2);
      ctx.fill();

      // Concentric rings
      const ringCount = 4;
      for (let i = 1; i <= ringCount; i++) {
        const frac = i / ringCount;
        // Pulse a little — every ring drifts slowly outward and fades.
        const pulse = ((t * 0.35 + i * 0.25) % 1);
        const r = maxR * frac * (0.98 + pulse * 0.04);
        ctx.beginPath();
        ctx.arc(cx, cy, r, 0, Math.PI * 2);
        ctx.strokeStyle = `rgba(189, 158, 255, ${0.14 + 0.06 * (1 - pulse)})`;
        ctx.lineWidth = 1;
        ctx.stroke();
      }

      // Crosshairs
      ctx.strokeStyle = "rgba(189, 158, 255, 0.10)";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(cx - maxR, cy);
      ctx.lineTo(cx + maxR, cy);
      ctx.moveTo(cx, cy - maxR);
      ctx.lineTo(cx, cy + maxR);
      ctx.stroke();

      // Sweep line (rotates every 3.2s).
      const sweepAngle = (t / 3.2) * Math.PI * 2;
      const grad = ctx.createLinearGradient(
        cx,
        cy,
        cx + Math.cos(sweepAngle - Math.PI / 2) * maxR,
        cy + Math.sin(sweepAngle - Math.PI / 2) * maxR,
      );
      grad.addColorStop(0, "rgba(189, 158, 255, 0.65)");
      grad.addColorStop(0.7, "rgba(189, 158, 255, 0.15)");
      grad.addColorStop(1, "rgba(189, 158, 255, 0)");
      ctx.strokeStyle = grad;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(cx, cy);
      ctx.lineTo(
        cx + Math.cos(sweepAngle - Math.PI / 2) * maxR,
        cy + Math.sin(sweepAngle - Math.PI / 2) * maxR,
      );
      ctx.stroke();

      // Sweep wedge afterglow (small filled triangle behind the line).
      const wedgeAngle = 0.35;
      ctx.beginPath();
      ctx.moveTo(cx, cy);
      const wedgeGrad = ctx.createRadialGradient(cx, cy, 0, cx, cy, maxR);
      wedgeGrad.addColorStop(0, "rgba(189, 158, 255, 0.18)");
      wedgeGrad.addColorStop(1, "rgba(189, 158, 255, 0)");
      ctx.fillStyle = wedgeGrad;
      ctx.arc(
        cx,
        cy,
        maxR,
        sweepAngle - Math.PI / 2 - wedgeAngle,
        sweepAngle - Math.PI / 2,
      );
      ctx.closePath();
      ctx.fill();

      // Center orb — "you"
      const orbGrad = ctx.createRadialGradient(cx, cy, 0, cx, cy, 14);
      orbGrad.addColorStop(0, "rgba(230, 210, 255, 0.95)");
      orbGrad.addColorStop(0.6, "rgba(189, 158, 255, 0.55)");
      orbGrad.addColorStop(1, "rgba(189, 158, 255, 0)");
      ctx.fillStyle = orbGrad;
      ctx.beginPath();
      ctx.arc(cx, cy, 14, 0, Math.PI * 2);
      ctx.fill();

      // Peer blips
      for (const item of positioned) {
        const angle = item.angle - Math.PI / 2;
        const r = maxR * item.radiusFrac;
        const px = cx + Math.cos(angle) * r;
        const py = cy + Math.sin(angle) * r;

        // Detect if the sweep is currently over this blip (within
        // 0.35 rad behind the leading edge). Boost brightness if so.
        const rel = (sweepAngle - item.angle + Math.PI * 4) % (Math.PI * 2);
        const litness = rel < 0.5 ? 1 - rel / 0.5 : 0;

        const isHovered = hovered === item.peer.id;
        const isActive = item.peer.relationship === "active";
        const color = isActive
          ? "rgba(140, 230, 175,"
          : "rgba(189, 158, 255,";

        // Halo
        const haloAlpha = 0.15 + 0.35 * litness + (isHovered ? 0.35 : 0);
        const haloRadius = 12 + (isHovered ? 4 : 0);
        const halo = ctx.createRadialGradient(px, py, 0, px, py, haloRadius);
        halo.addColorStop(0, `${color} ${Math.min(1, haloAlpha)})`);
        halo.addColorStop(1, `${color} 0)`);
        ctx.fillStyle = halo;
        ctx.beginPath();
        ctx.arc(px, py, haloRadius, 0, Math.PI * 2);
        ctx.fill();

        // Core dot
        ctx.fillStyle = `${color} ${0.9 + 0.1 * litness})`;
        ctx.beginPath();
        ctx.arc(px, py, isHovered ? 4 : 3, 0, Math.PI * 2);
        ctx.fill();
      }

      raf = requestAnimationFrame(draw);
    };
    draw();
    return () => cancelAnimationFrame(raf);
  }, [size, positioned, hovered]);

  // Hit-test the canvas so hover/click on blips work.
  const onMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    const cx = size.w / 2;
    const cy = size.h / 2;
    const maxR = Math.min(size.w, size.h * 2) * 0.45;
    let closest: { id: string; dist: number } | null = null;
    for (const item of positioned) {
      const angle = item.angle - Math.PI / 2;
      const r = maxR * item.radiusFrac;
      const px = cx + Math.cos(angle) * r;
      const py = cy + Math.sin(angle) * r;
      const d = Math.hypot(x - px, y - py);
      if (d < 14 && (!closest || d < closest.dist)) {
        closest = { id: item.peer.id, dist: d };
      }
    }
    setHovered(closest?.id ?? null);
    canvas.style.cursor = closest ? "pointer" : "default";
  };

  const onClick = () => {
    if (!hovered) return;
    const item = positioned.find((p) => p.peer.id === hovered);
    if (item) onPeerClick(item.peer);
  };

  const hoveredPeer = positioned.find((p) => p.peer.id === hovered)?.peer;

  return (
    <div
      ref={wrapRef}
      className="relative rounded-2xl overflow-hidden"
      style={{
        height: size.h,
        background:
          "linear-gradient(180deg, rgba(20,16,36,0.75), rgba(12,12,20,0.9))",
        border: "1px solid rgba(189, 158, 255, 0.22)",
        boxShadow: "inset 0 0 60px rgba(140, 105, 235, 0.14)",
      }}
    >
      <canvas
        ref={canvasRef}
        onMouseMove={onMouseMove}
        onMouseLeave={() => setHovered(null)}
        onClick={onClick}
        style={{ width: "100%", height: "100%", display: "block" }}
      />
      {/* Tooltip */}
      <AnimatePresence>
        {hoveredPeer && (
          <motion.div
            key={`t-${hoveredPeer.id}`}
            initial={{ opacity: 0, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 4 }}
            transition={{ duration: 0.18 }}
            className="absolute top-3 left-1/2 pointer-events-none rounded-lg px-2.5 py-1.5"
            style={{
              transform: "translateX(-50%)",
              background: "rgba(14, 12, 20, 0.88)",
              border: `1px solid ${
                hoveredPeer.relationship === "active"
                  ? "rgba(140, 230, 175, 0.45)"
                  : "rgba(189, 158, 255, 0.42)"
              }`,
              color: "rgba(240, 240, 246, 0.95)",
              backdropFilter: "blur(6px)",
              fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
              fontSize: 11.5,
            }}
          >
            {hoveredPeer.displayName}
            <span
              className="ml-2 text-[10px]"
              style={{
                color:
                  hoveredPeer.relationship === "active"
                    ? "rgba(140, 230, 175, 0.85)"
                    : "rgba(189, 158, 255, 0.75)",
              }}
            >
              {hoveredPeer.relationship === "active"
                ? "connected"
                : hoveredPeer.relationship === "pending"
                ? "invite pending"
                : hoveredPeer.source === "ble"
                ? "ble · click to invite"
                : "mdns · click to invite"}
            </span>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function PeerListRow({
  peer,
  highlighted,
  onHover,
  onPair,
  onSendFile,
}: {
  peer: UnifiedPeer;
  highlighted: boolean;
  onHover: (id: string | null) => void;
  onPair: () => void;
  onSendFile: () => void;
}) {
  const connected = peer.relationship === "active";
  return (
    <div
      onMouseEnter={() => onHover(peer.id)}
      onMouseLeave={() => onHover(null)}
      className="flex items-center gap-3 rounded-lg px-3 py-2 transition-colors"
      style={{
        background: highlighted
          ? "rgba(189, 158, 255, 0.08)"
          : "rgba(255, 255, 255, 0.02)",
        border: `1px solid ${
          highlighted ? "rgba(189, 158, 255, 0.35)" : "rgba(255, 255, 255, 0.06)"
        }`,
      }}
    >
      <div
        className="w-9 h-9 rounded-full flex items-center justify-center shrink-0"
        style={{
          background: connected
            ? "linear-gradient(140deg, rgba(140,230,175,0.32), rgba(90,190,130,0.32))"
            : "linear-gradient(140deg, rgba(189,158,255,0.32), rgba(140,105,235,0.32))",
          border: `1px solid ${
            connected ? "rgba(140, 230, 175, 0.55)" : "rgba(189, 158, 255, 0.55)"
          }`,
          color: "rgba(250, 248, 255, 0.95)",
          fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
          fontSize: 11,
        }}
      >
        {initialsFor(peer.displayName)}
      </div>
      <div className="min-w-0 flex-1">
        <div
          className="text-[13px] font-medium truncate"
          style={{ color: "rgba(240, 240, 246, 0.94)" }}
        >
          {peer.displayName}
        </div>
        <div
          className="text-[10.5px] font-mono"
          style={{
            color: connected
              ? "rgba(140, 230, 175, 0.85)"
              : "rgba(189, 158, 255, 0.75)",
          }}
        >
          {peer.source} · {connected ? "connected" : "nearby"}
          {peer.rssi != null && ` · ${peer.rssi} dBm`}
        </div>
      </div>
      {connected ? (
        <PillButton onClick={onSendFile} icon={<FileIcon />} tone="green">
          send file
        </PillButton>
      ) : (
        <PillButton onClick={onPair} tone="purple">
          invite
        </PillButton>
      )}
    </div>
  );
}

// ─── Circles tab ───────────────────────────────────────────────────

function CirclesPane({
  circles,
  onCreate,
  onJoin,
  onLeave,
  onDelete,
  onFlash,
  onError,
  contacts,
}: {
  circles: Circle[];
  onCreate: (name: string, description?: string) => Promise<void>;
  onJoin: (code: string) => Promise<void>;
  onLeave: (c: Circle) => Promise<void>;
  onDelete: (c: Circle) => Promise<void>;
  onFlash: (m: string) => void;
  onError: (m: string) => void;
  contacts: CircleContact[];
}) {
  return (
    <div className="mt-4 flex flex-col gap-4">
      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
        <InlineCircleCreate onSubmit={onCreate} />
        <InlineCircleJoin onSubmit={onJoin} />
      </div>
      <div>
        <SectionHeader
          label="Your circles"
          hint={
            circles.length > 0
              ? `${circles.length} ${circles.length === 1 ? "group" : "groups"}`
              : undefined
          }
        />
        <div className="mt-2 flex flex-col gap-1.5">
          {circles.length === 0 && (
            <div
              className="rounded-xl px-4 py-6 text-center text-[12.5px]"
              style={{
                background: "rgba(255, 255, 255, 0.02)",
                border: "1px dashed rgba(255, 255, 255, 0.10)",
                color: "rgba(236, 236, 241, 0.62)",
              }}
            >
              No circles yet. Create one above or join with a code.
            </div>
          )}
          {circles.map((c) => (
            <CircleRow
              key={c.id}
              circle={c}
              onLeave={() => onLeave(c)}
              onDelete={() => onDelete(c)}
              onFlash={onFlash}
              onError={onError}
            />
          ))}
        </div>
      </div>
      {contacts.length > 0 && (
        <div>
          <SectionHeader
            label="People across your circles"
            hint={`${contacts.length} ${contacts.length === 1 ? "person" : "people"}`}
          />
          <div className="mt-2 flex flex-col gap-1.5">
            {contacts.map((cc) => (
              <div
                key={cc.id}
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
                  {initialsFor(cc.name ?? cc.email)}
                </div>
                <div className="min-w-0 flex-1">
                  <div
                    className="text-[13px] font-medium truncate"
                    style={{ color: "rgba(240, 240, 246, 0.94)" }}
                  >
                    {cc.name ?? cc.email}
                  </div>
                  <div
                    className="text-[10.5px] font-mono"
                    style={{ color: "rgba(189, 158, 255, 0.75)" }}
                  >
                    via circle
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function InlineCircleCreate({
  onSubmit,
}: {
  onSubmit: (name: string, description?: string) => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || busy) return;
    setBusy(true);
    try {
      await onSubmit(name, description || undefined);
      setName("");
      setDescription("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      onSubmit={submit}
      className="rounded-xl p-4 flex flex-col gap-2"
      style={{
        background:
          "linear-gradient(140deg, rgba(189,158,255,0.10), rgba(140,105,235,0.06))",
        border: "1px solid rgba(189, 158, 255, 0.30)",
      }}
    >
      <div
        className="text-[10px] uppercase tracking-[0.22em] font-mono"
        style={{ color: "rgba(189, 158, 255, 0.85)" }}
      >
        // create a circle
      </div>
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        disabled={busy}
        maxLength={80}
        placeholder="Family, Book club, team-marketing…"
        className="bg-white/[0.03] border rounded-md px-2.5 py-1.5 text-[13px] text-white placeholder:text-white/35 focus:outline-none focus:border-[#bd9eff]/50 disabled:opacity-50"
        style={{ borderColor: "rgba(255, 255, 255, 0.10)" }}
      />
      <input
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        disabled={busy}
        maxLength={200}
        placeholder="Description (optional)"
        className="bg-white/[0.03] border rounded-md px-2.5 py-1.5 text-[12px] text-white placeholder:text-white/35 focus:outline-none focus:border-[#bd9eff]/50 disabled:opacity-50"
        style={{ borderColor: "rgba(255, 255, 255, 0.08)" }}
      />
      <button
        type="submit"
        disabled={busy || !name.trim()}
        className="self-end text-[11px] uppercase tracking-[0.16em] font-mono px-3 py-1.5 rounded-md disabled:opacity-40"
        style={{
          background: "rgba(189, 158, 255, 0.16)",
          color: "rgb(189, 158, 255)",
          border: "1px solid rgba(189, 158, 255, 0.45)",
        }}
      >
        {busy ? "creating…" : "create"}
      </button>
    </form>
  );
}

function InlineCircleJoin({
  onSubmit,
}: {
  onSubmit: (code: string) => Promise<void>;
}) {
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!code.trim() || busy) return;
    setBusy(true);
    try {
      await onSubmit(code);
      setCode("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      onSubmit={submit}
      className="rounded-xl p-4 flex flex-col gap-2"
      style={{
        background:
          "linear-gradient(140deg, rgba(110,196,232,0.08), rgba(90,150,210,0.05))",
        border: "1px solid rgba(110, 196, 232, 0.28)",
      }}
    >
      <div
        className="text-[10px] uppercase tracking-[0.22em] font-mono"
        style={{ color: "rgba(140, 200, 235, 0.85)" }}
      >
        // join a circle
      </div>
      <input
        value={code}
        onChange={(e) => setCode(e.target.value.toUpperCase())}
        disabled={busy}
        maxLength={12}
        spellCheck={false}
        autoCapitalize="characters"
        autoComplete="off"
        placeholder="ABCD2345"
        className="bg-white/[0.03] border rounded-md px-2.5 py-2 text-[17px] font-mono text-center tracking-[0.28em] text-white placeholder:text-white/25 focus:outline-none focus:border-[#6ec4e8]/60 disabled:opacity-50"
        style={{ borderColor: "rgba(255, 255, 255, 0.10)" }}
      />
      <button
        type="submit"
        disabled={busy || !code.trim()}
        className="self-end text-[11px] uppercase tracking-[0.16em] font-mono px-3 py-1.5 rounded-md disabled:opacity-40"
        style={{
          background: "rgba(110, 196, 232, 0.14)",
          color: "rgb(140, 200, 235)",
          border: "1px solid rgba(110, 196, 232, 0.45)",
        }}
      >
        {busy ? "joining…" : "join"}
      </button>
    </form>
  );
}

function CircleRow({
  circle,
  onLeave,
  onDelete,
  onFlash,
  onError,
}: {
  circle: Circle;
  onLeave: () => void;
  onDelete: () => void;
  onFlash: (msg: string) => void;
  onError: (msg: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [members, setMembers] = useState<CircleMember[] | null>(null);
  const [copied, setCopied] = useState(false);

  const loadMembers = async () => {
    if (members) return;
    try {
      const list = await circlesMembers(circle.id);
      setMembers(list);
    } catch (e) {
      onError(String(e));
    }
  };

  const toggle = async () => {
    if (!expanded) await loadMembers();
    setExpanded((v) => !v);
  };

  const copyCode = () => {
    void navigator.clipboard.writeText(circle.join_code);
    onFlash(`Copied "${circle.join_code}"`);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  const isOwner = circle.role === "owner";

  return (
    <div
      className="rounded-lg overflow-hidden"
      style={{
        background: "rgba(255, 255, 255, 0.02)",
        border: "1px solid rgba(255, 255, 255, 0.06)",
      }}
    >
      <div className="flex items-center gap-3 px-3 py-2">
        <button
          onClick={toggle}
          className="w-8 h-8 rounded-md flex items-center justify-center shrink-0 transition-colors"
          style={{
            background: "rgba(189, 158, 255, 0.14)",
            border: "1px solid rgba(189, 158, 255, 0.30)",
            color: "rgba(240, 240, 246, 0.85)",
          }}
          title={expanded ? "Collapse" : "Show members"}
        >
          <motion.span
            animate={{ rotate: expanded ? 90 : 0 }}
            transition={{ duration: 0.18 }}
            className="inline-flex"
          >
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M9 6l6 6-6 6" />
            </svg>
          </motion.span>
        </button>
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span
              className="text-[13.5px] font-medium truncate"
              style={{ color: "rgba(240, 240, 246, 0.94)" }}
            >
              {circle.name}
            </span>
            <span
              className="text-[10px] font-mono"
              style={{ color: "rgba(189, 158, 255, 0.75)" }}
            >
              {isOwner ? "owner" : "member"}
            </span>
          </div>
          {circle.description && (
            <div
              className="text-[11.5px] mt-0.5 leading-snug truncate"
              style={{ color: "rgba(236, 236, 241, 0.62)" }}
            >
              {circle.description}
            </div>
          )}
        </div>
        <div className="flex items-center gap-1.5 shrink-0">
          <button
            onClick={copyCode}
            className="text-[10.5px] uppercase tracking-[0.16em] font-mono px-2.5 py-1 rounded-md"
            style={{
              background: "rgba(189, 158, 255, 0.10)",
              color: "rgb(189, 158, 255)",
              border: "1px solid rgba(189, 158, 255, 0.35)",
            }}
            title="Copy join code"
          >
            {copied ? "copied ✓" : circle.join_code}
          </button>
          <span
            className="text-[10.5px] font-mono"
            style={{ color: "rgba(236, 236, 241, 0.55)" }}
            title={`${circle.member_count} member${circle.member_count === 1 ? "" : "s"}`}
          >
            {circle.member_count}
          </span>
          {isOwner ? (
            <button
              onClick={onDelete}
              className="text-[10.5px] uppercase tracking-[0.16em] font-mono px-2.5 py-1 rounded-md"
              style={{
                background: "rgba(255, 255, 255, 0.04)",
                color: "rgba(255, 155, 155, 0.75)",
                border: "1px solid rgba(255, 155, 155, 0.30)",
              }}
              title="Delete circle (owner only)"
            >
              delete
            </button>
          ) : (
            <button
              onClick={onLeave}
              className="text-[10.5px] uppercase tracking-[0.16em] font-mono px-2.5 py-1 rounded-md"
              style={{
                background: "rgba(255, 255, 255, 0.04)",
                color: "rgba(236, 236, 241, 0.65)",
                border: "1px solid rgba(255, 255, 255, 0.10)",
              }}
            >
              leave
            </button>
          )}
        </div>
      </div>
      <AnimatePresence>
        {expanded && (
          <motion.div
            key="members"
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
            className="overflow-hidden"
            style={{ borderTop: "1px solid rgba(255, 255, 255, 0.06)" }}
          >
            <div className="px-3 py-2 flex flex-col gap-1.5">
              {members === null ? (
                <div
                  className="text-[11.5px] font-mono"
                  style={{ color: "rgba(236, 236, 241, 0.55)" }}
                >
                  loading members…
                </div>
              ) : members.length === 0 ? (
                <div
                  className="text-[11.5px]"
                  style={{ color: "rgba(236, 236, 241, 0.55)" }}
                >
                  Just you so far. Share the join code to bring people in.
                </div>
              ) : (
                members.map((m) => (
                  <div
                    key={m.id}
                    className="flex items-center gap-2 text-[12.5px]"
                    style={{ color: "rgba(236, 236, 241, 0.88)" }}
                  >
                    <div
                      className="w-6 h-6 rounded-full flex items-center justify-center shrink-0"
                      style={{
                        background: "rgba(189, 158, 255, 0.14)",
                        border: "1px solid rgba(189, 158, 255, 0.28)",
                        color: "rgba(250, 248, 255, 0.9)",
                        fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
                        fontSize: 9,
                      }}
                    >
                      {initialsFor(m.name ?? m.email)}
                    </div>
                    <span className="truncate">{m.name ?? m.email}</span>
                    {m.role === "owner" && (
                      <span
                        className="text-[9.5px] uppercase tracking-[0.14em] font-mono"
                        style={{ color: "rgba(189, 158, 255, 0.75)" }}
                      >
                        owner
                      </span>
                    )}
                  </div>
                ))
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

// ─── Contacts tab ──────────────────────────────────────────────────

function ContactsPane({
  incoming,
  active,
  outgoing,
  onAccept,
  onRevoke,
  onInvite,
  onSendFile,
}: {
  incoming: T2tRelationship[];
  active: T2tRelationship[];
  outgoing: T2tRelationship[];
  onAccept: (r: T2tRelationship) => void;
  onRevoke: (r: T2tRelationship) => void;
  onInvite: (email: string, reason?: string) => Promise<void>;
  onSendFile: (r: T2tRelationship) => void;
}) {
  return (
    <div className="mt-4 flex flex-col gap-4">
      <InlineInviteForm onSubmit={onInvite} />
      {incoming.length + outgoing.length + active.length === 0 && (
        <div
          className="rounded-xl px-4 py-6 text-center text-[12.5px]"
          style={{
            background: "rgba(255, 255, 255, 0.02)",
            border: "1px dashed rgba(255, 255, 255, 0.10)",
            color: "rgba(236, 236, 241, 0.62)",
          }}
        >
          No contacts yet. Send an invite above, or pair with a nearby
          Travis on the Discover tab.
        </div>
      )}
      {incoming.length > 0 && (
        <div>
          <SectionHeader label="Waiting for you" hint={`${incoming.length}`} />
          <div className="mt-2 flex flex-col gap-1.5">
            {incoming.map((r) => (
              <RelationshipRow
                key={r.id}
                r={r}
                variant="incoming"
                onAccept={() => onAccept(r)}
                onRevoke={() => onRevoke(r)}
                onSendFile={() => onSendFile(r)}
              />
            ))}
          </div>
        </div>
      )}
      {active.length > 0 && (
        <div>
          <SectionHeader label="Connected" hint={`${active.length}`} />
          <div className="mt-2 flex flex-col gap-1.5">
            {active.map((r) => (
              <RelationshipRow
                key={r.id}
                r={r}
                variant="active"
                onRevoke={() => onRevoke(r)}
                onSendFile={() => onSendFile(r)}
              />
            ))}
          </div>
        </div>
      )}
      {outgoing.length > 0 && (
        <div>
          <SectionHeader label="Sent invites" hint={`${outgoing.length}`} />
          <div className="mt-2 flex flex-col gap-1.5">
            {outgoing.map((r) => (
              <RelationshipRow
                key={r.id}
                r={r}
                variant="outgoing"
                onRevoke={() => onRevoke(r)}
                onSendFile={() => onSendFile(r)}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function InlineInviteForm({
  onSubmit,
}: {
  onSubmit: (email: string, reason?: string) => Promise<void>;
}) {
  const [email, setEmail] = useState("");
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!email.trim() || busy) return;
    setBusy(true);
    try {
      await onSubmit(email, reason);
      setEmail("");
      setReason("");
    } finally {
      setBusy(false);
    }
  };
  return (
    <form
      onSubmit={submit}
      className="rounded-xl p-4 flex flex-col gap-2"
      style={{
        background:
          "linear-gradient(140deg, rgba(189,158,255,0.10), rgba(140,105,235,0.06))",
        border: "1px solid rgba(189, 158, 255, 0.30)",
      }}
    >
      <div
        className="text-[10px] uppercase tracking-[0.22em] font-mono"
        style={{ color: "rgba(189, 158, 255, 0.85)" }}
      >
        // invite by email
      </div>
      <div className="flex gap-2">
        <input
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          disabled={busy}
          placeholder="their@email.com"
          className="flex-1 bg-white/[0.03] border rounded-md px-2.5 py-1.5 text-[13px] text-white placeholder:text-white/40 focus:outline-none focus:border-[#bd9eff]/50 disabled:opacity-50"
          style={{ borderColor: "rgba(255, 255, 255, 0.10)" }}
        />
        <button
          type="submit"
          disabled={busy || !email.trim()}
          className="shrink-0 text-[11px] uppercase tracking-[0.16em] font-mono px-3 py-1.5 rounded-md disabled:opacity-40"
          style={{
            background: "rgba(189, 158, 255, 0.16)",
            color: "rgb(189, 158, 255)",
            border: "1px solid rgba(189, 158, 255, 0.45)",
          }}
        >
          {busy ? "sending…" : "invite"}
        </button>
      </div>
      <input
        value={reason}
        onChange={(e) => setReason(e.target.value)}
        disabled={busy}
        placeholder="Optional note ('for planning our trip', 'for the campaign')"
        className="bg-white/[0.03] border rounded-md px-2.5 py-1.5 text-[12px] text-white placeholder:text-white/40 focus:outline-none focus:border-[#bd9eff]/50 disabled:opacity-50"
        style={{ borderColor: "rgba(255, 255, 255, 0.08)" }}
      />
    </form>
  );
}

function RelationshipRow({
  r,
  variant,
  onAccept,
  onRevoke,
  onSendFile,
}: {
  r: T2tRelationship;
  variant: "incoming" | "outgoing" | "active";
  onAccept?: () => void;
  onRevoke: () => void;
  onSendFile?: () => void;
}) {
  const name = r.other_name ?? r.other_email ?? "unknown";
  const detail =
    variant === "incoming"
      ? "invited you"
      : variant === "outgoing"
      ? "invite pending"
      : r.other_email ?? "connected";

  const accent =
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
        <div className="text-[10.5px] font-mono" style={{ color: accent }}>
          {detail}
        </div>
      </div>
      {variant === "incoming" && onAccept && (
        <PillButton onClick={onAccept} tone="green">
          accept
        </PillButton>
      )}
      {variant === "active" && onSendFile && (
        <PillButton onClick={onSendFile} icon={<FileIcon />} tone="green">
          send file
        </PillButton>
      )}
      <PillButton onClick={onRevoke} tone="muted">
        {variant === "outgoing" || variant === "incoming" ? "cancel" : "remove"}
      </PillButton>
    </div>
  );
}

// ─── Pair tab ──────────────────────────────────────────────────────

function PairPane({
  pendingToken,
  onRedeem,
}: {
  pendingToken: string;
  onRedeem: (token: string) => Promise<void>;
}) {
  return (
    <div className="mt-4 grid grid-cols-1 md:grid-cols-2 gap-4">
      <SharePairPane />
      <RedeemPairPane initialToken={pendingToken} onRedeem={onRedeem} />
    </div>
  );
}

function SharePairPane() {
  const [token, setToken] = useState<PairToken | null>(null);
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState<"" | "code" | "link">("");
  const [err, setErr] = useState<string | null>(null);

  const issue = useCallback(async () => {
    setBusy(true);
    setErr(null);
    try {
      const t = await t2tPairCreateToken();
      setToken(t);
      const png = await QRCode.toDataURL(t.deep_link, {
        errorCorrectionLevel: "M",
        margin: 1,
        width: 220,
        color: { dark: "#EDE7FF", light: "#00000000" },
      });
      setQrDataUrl(png);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    if (!token) void issue();
  }, [token, issue]);

  const copyCode = () => {
    if (!token) return;
    void navigator.clipboard.writeText(token.token);
    setCopied("code");
    window.setTimeout(() => setCopied(""), 1500);
  };
  const copyLink = () => {
    if (!token) return;
    void navigator.clipboard.writeText(token.deep_link);
    setCopied("link");
    window.setTimeout(() => setCopied(""), 1500);
  };

  return (
    <div
      className="rounded-xl p-4 flex flex-col items-center gap-3"
      style={{
        background:
          "linear-gradient(160deg, rgba(189,158,255,0.10), rgba(140,105,235,0.06) 55%, transparent)",
        border: "1px solid rgba(189, 158, 255, 0.32)",
      }}
    >
      <div
        className="text-[10px] uppercase tracking-[0.22em] font-mono self-start"
        style={{ color: "rgba(189, 158, 255, 0.85)" }}
      >
        // share
      </div>
      <div
        className="rounded-xl p-3 flex items-center justify-center"
        style={{
          background: "rgba(255, 255, 255, 0.03)",
          border: "1px solid rgba(189, 158, 255, 0.22)",
          width: 220,
          height: 220,
        }}
      >
        {qrDataUrl ? (
          <img
            src={qrDataUrl}
            alt="Pair code QR"
            width={196}
            height={196}
            style={{ imageRendering: "pixelated" }}
          />
        ) : (
          <div
            className="text-[11px] font-mono"
            style={{ color: "rgba(236, 236, 241, 0.6)" }}
          >
            {err ?? (busy ? "issuing…" : "")}
          </div>
        )}
      </div>
      {token && (
        <>
          <button
            onClick={copyCode}
            className="text-[20px] font-mono px-4 py-1.5 rounded-lg tracking-[0.24em]"
            style={{
              background: "rgba(189, 158, 255, 0.12)",
              color: "rgb(240, 240, 246)",
              border: "1px solid rgba(189, 158, 255, 0.40)",
            }}
            title="Copy the code"
          >
            {copied === "code" ? "copied ✓" : token.token}
          </button>
          <div className="flex items-center gap-2">
            <button
              onClick={copyLink}
              className="text-[10.5px] uppercase tracking-[0.18em] font-mono px-2.5 py-1 rounded-md"
              style={{
                background: "rgba(255, 255, 255, 0.04)",
                color: "rgba(236, 236, 241, 0.75)",
                border: "1px solid rgba(255, 255, 255, 0.10)",
              }}
            >
              {copied === "link" ? "copied ✓" : "copy deep link"}
            </button>
            <button
              onClick={() => {
                setToken(null);
                setQrDataUrl(null);
              }}
              className="text-[10.5px] uppercase tracking-[0.18em] font-mono px-2.5 py-1 rounded-md"
              style={{
                background: "rgba(255, 255, 255, 0.04)",
                color: "rgba(236, 236, 241, 0.75)",
                border: "1px solid rgba(255, 255, 255, 0.10)",
              }}
              title="Issue a new code"
            >
              new code
            </button>
          </div>
          <p
            className="text-[11px] text-center leading-relaxed"
            style={{ color: "rgba(236, 236, 241, 0.55)" }}
          >
            Scan the QR or share the code. Expires in 24 hours.
          </p>
        </>
      )}
    </div>
  );
}

function RedeemPairPane({
  initialToken,
  onRedeem,
}: {
  initialToken: string;
  onRedeem: (token: string) => Promise<void>;
}) {
  const [value, setValue] = useState(initialToken);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const setPendingPairToken = useAppStore((s) => s.setPendingPairToken);

  useEffect(() => {
    if (initialToken) {
      setValue(initialToken);
      // Auto-redeem when a deep link populates the token.
      void (async () => {
        try {
          setBusy(true);
          await onRedeem(initialToken);
          setValue("");
        } finally {
          setBusy(false);
          setPendingPairToken(null);
        }
      })();
    }
  }, [initialToken, onRedeem, setPendingPairToken]);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!value.trim() || busy) return;
    setBusy(true);
    try {
      await onRedeem(value);
      setValue("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      onSubmit={submit}
      className="rounded-xl p-4 flex flex-col gap-3 h-full"
      style={{
        background:
          "linear-gradient(160deg, rgba(110,196,232,0.08), rgba(90,150,210,0.04) 55%, transparent)",
        border: "1px solid rgba(110, 196, 232, 0.28)",
      }}
    >
      <div
        className="text-[10px] uppercase tracking-[0.22em] font-mono"
        style={{ color: "rgba(140, 200, 235, 0.85)" }}
      >
        // enter a pair code
      </div>
      <p
        className="text-[12px] leading-relaxed"
        style={{ color: "rgba(236, 236, 241, 0.68)" }}
      >
        Ask the other person for their code (from their Pair tab) or
        scan their QR. Type it here and Travis handles the rest.
      </p>
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
        className="w-full bg-white/[0.03] border rounded-md px-3 py-2.5 text-[19px] font-mono text-center tracking-[0.30em] text-white placeholder:text-white/25 focus:outline-none focus:border-[#6ec4e8]/60 disabled:opacity-50"
        style={{ borderColor: "rgba(255, 255, 255, 0.12)" }}
      />
      <button
        type="submit"
        disabled={busy || !value.trim()}
        className="self-end text-[11px] uppercase tracking-[0.16em] font-mono px-4 py-1.5 rounded-md disabled:opacity-40"
        style={{
          background: "rgba(110, 196, 232, 0.14)",
          color: "rgb(140, 200, 235)",
          border: "1px solid rgba(110, 196, 232, 0.45)",
        }}
      >
        {busy ? "pairing…" : "pair"}
      </button>
    </form>
  );
}

// ─── Shared bits ───────────────────────────────────────────────────

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

function PillButton({
  children,
  onClick,
  icon,
  tone = "purple",
}: {
  children: React.ReactNode;
  onClick: () => void;
  icon?: React.ReactNode;
  tone?: "purple" | "green" | "muted";
}) {
  const themes = {
    purple: {
      bg: "rgba(189, 158, 255, 0.14)",
      border: "rgba(189, 158, 255, 0.45)",
      color: "rgb(189, 158, 255)",
    },
    green: {
      bg: "rgba(140, 230, 175, 0.14)",
      border: "rgba(140, 230, 175, 0.45)",
      color: "rgb(140, 230, 175)",
    },
    muted: {
      bg: "rgba(255, 255, 255, 0.04)",
      border: "rgba(255, 255, 255, 0.10)",
      color: "rgba(236, 236, 241, 0.65)",
    },
  }[tone];
  return (
    <button
      onClick={onClick}
      className="text-[10.5px] uppercase tracking-[0.16em] font-mono px-2.5 py-1 rounded-md inline-flex items-center gap-1"
      style={{
        background: themes.bg,
        color: themes.color,
        border: `1px solid ${themes.border}`,
      }}
    >
      {icon}
      {children}
    </button>
  );
}

function initialsFor(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "??";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

function hashString(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = (h * 16777619) >>> 0;
  }
  return h >>> 0;
}

// ─── Icons ─────────────────────────────────────────────────────────

function RadarIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="12" r="5" />
      <path d="M12 12l6-6" />
      <circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none" />
    </svg>
  );
}

function CircleIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <circle cx="9" cy="10" r="4" />
      <circle cx="16" cy="10" r="4" />
      <path d="M4 20c0-2.2 1.8-4 4-4M20 20c0-2.2-1.8-4-4-4" />
    </svg>
  );
}

function PeopleIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <circle cx="12" cy="8" r="3.5" />
      <path d="M4 20a8 8 0 0 1 16 0" />
    </svg>
  );
}

function QrIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <rect x="3" y="3" width="7" height="7" />
      <rect x="14" y="3" width="7" height="7" />
      <rect x="3" y="14" width="7" height="7" />
      <path d="M14 14h3v3h-3zM19 14h2M14 19h2M19 21h2" />
    </svg>
  );
}

function FileIcon() {
  return (
    <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
      <path d="M14 3v5h5" />
    </svg>
  );
}
