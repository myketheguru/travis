/**
 * SentryConsentModal — v0.28.45.
 *
 * The sleek explainer/consent gate that appears when a user tries to
 * turn Sentry mode on for the first time (or when the capture scope
 * expands and they need to re-consent). Explicitly enumerates:
 *
 *   - What Travis captures
 *   - Where it's stored
 *   - What (if anything) leaves the device
 *   - Why (so Travis can serve you better in your actual workflow)
 *   - What Travis will NEVER capture
 *   - How to pause / purge
 *
 * Two actions: "I agree, turn it on" (proceeds) and "Not now".
 * Requires the user to actively read (mouse must have scrolled through
 * or reached the bottom — soft gate, we track scroll state) before
 * the primary action enables. Not perfect but strongly signals intent.
 */
import { useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";

// v0.28.52 — bumped from 1 to 2 because the capture scope now
// actually includes local screenshots (v1 promised them as "coming
// soon"). Existing users see the consent modal again before
// screenshots start.
export const SENTRY_CONSENT_VERSION = 2;

/// Persist the accepted consent version in localStorage so we can
/// re-prompt when the scope expands (bump the constant above).
const KEY = "travis.sentry.consentVersion";

export function loadSentryConsentVersion(): number {
  try {
    if (typeof localStorage === "undefined") return 0;
    return Number(localStorage.getItem(KEY) ?? 0) || 0;
  } catch {
    return 0;
  }
}

export function saveSentryConsentVersion(v: number) {
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(KEY, String(v));
    }
  } catch {
    /* private mode */
  }
}

export function hasCurrentSentryConsent(): boolean {
  return loadSentryConsentVersion() >= SENTRY_CONSENT_VERSION;
}

export function SentryConsentModal({
  open,
  onAgree,
  onCancel,
}: {
  open: boolean;
  onAgree: () => void;
  onCancel: () => void;
}) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [readEnough, setReadEnough] = useState(false);

  // Close on Escape
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onCancel]);

  // Reset the scroll-through gate every time the modal (re)opens.
  useEffect(() => {
    if (open) setReadEnough(false);
  }, [open]);

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    const remaining = el.scrollHeight - el.scrollTop - el.clientHeight;
    if (remaining < 30) setReadEnough(true);
  };

  const handleAgree = () => {
    saveSentryConsentVersion(SENTRY_CONSENT_VERSION);
    onAgree();
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          key="consent-backdrop"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
          className="fixed inset-0 z-50 flex items-center justify-center px-4"
          style={{
            background: "rgba(0, 0, 0, 0.68)",
            backdropFilter: "blur(6px)",
          }}
          onClick={onCancel}
        >
          <motion.div
            key="consent-card"
            initial={{ opacity: 0, y: 14, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 8, scale: 0.98 }}
            transition={{ duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
            className="relative w-full max-w-lg rounded-2xl overflow-hidden shadow-2xl flex flex-col"
            style={{
              background: "rgb(14, 12, 20)",
              border: "1px solid rgba(189, 158, 255, 0.28)",
              maxHeight: "88vh",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            {/* Header */}
            <div className="px-6 pt-6 pb-3 shrink-0">
              <div
                className="text-[10px] uppercase tracking-[0.22em] font-mono mb-1.5"
                style={{ color: "rgba(189, 158, 255, 0.85)" }}
              >
                // sentry mode
              </div>
              <h2
                className="text-[20px] font-medium leading-tight"
                style={{ color: "rgb(240, 240, 246)" }}
              >
                Letting Travis observe your work
              </h2>
              <p
                className="text-[13px] mt-2 leading-relaxed"
                style={{ color: "rgba(236, 236, 241, 0.72)" }}
              >
                Read this carefully, then decide. You can pause or purge
                at any time.
              </p>
            </div>

            {/* Scrollable content */}
            <div
              ref={scrollRef}
              onScroll={onScroll}
              className="px-6 py-3 overflow-y-auto flex-1 flex flex-col gap-4"
              style={{ color: "rgba(236, 236, 241, 0.88)" }}
            >
              <Block title="What Travis captures">
                <ul className="mt-1.5 flex flex-col gap-1.5">
                  <Bullet>
                    Which app you're in and its window title — every
                    30 seconds, in the background.
                  </Bullet>
                  <Bullet>
                    A resized JPEG screenshot of your primary monitor
                    every five minutes. Long side capped at 1600px so
                    a full-workday rolling window is a few dozen
                    megabytes, not gigabytes.
                  </Bullet>
                  <Bullet>
                    Focus + interaction rhythm — how long you stay in
                    an app, when you switch, when you go idle.
                  </Bullet>
                </ul>
              </Block>

              <Block title="Why we're asking">
                <p className="mt-1.5 leading-relaxed">
                  You asked Travis to be genuinely useful in your
                  workflow — not just answer prompts. To do that,
                  Travis needs to know what your work actually looks
                  like. Sentry is what turns Travis from a chatbot
                  into an assistant that can proactively notice
                  something's wrong with your invoice, spot a
                  scheduling conflict, or resume where you left off.
                </p>
                <p className="mt-2 leading-relaxed">
                  This is an opt-in feature people specifically
                  requested. It's off for everyone by default.
                </p>
              </Block>

              <Block title="Where it lives">
                <ul className="mt-1.5 flex flex-col gap-1.5">
                  <Bullet>
                    <strong>Screenshots stay entirely on your device</strong>
                    {" "}for now — saved as JPEGs in Travis's local
                    app-data directory, rolling window of the most
                    recent 20 files. Nothing about the images leaves
                    your machine.
                  </Bullet>
                  <Bullet>
                    <strong>Window titles + app names</strong> are
                    batched to your Travis Cloud account so cloud-side
                    reasoning can help. Encrypted in transit, scoped
                    to your account, never shared with anyone else,
                    never used to train external models.
                  </Bullet>
                  <Bullet>
                    Cloud upload of a rolling window of screenshots is
                    coming next, and will require its own re-consent
                    before starting.
                  </Bullet>
                  <Bullet>
                    You can purge everything (local + cloud) any time
                    with a single button in Settings.
                  </Bullet>
                </ul>
              </Block>

              <Block title="What Travis will NEVER capture">
                <ul className="mt-1.5 flex flex-col gap-1.5">
                  <Bullet>Individual keystrokes.</Bullet>
                  <Bullet>Your clipboard contents.</Bullet>
                  <Bullet>Passwords, form fields, or fields marked
                    sensitive by your OS.</Bullet>
                  <Bullet>Anything from workspaces you've marked
                    Sensitive (Health, Therapy, Legal, Finance).</Bullet>
                </ul>
              </Block>

              <Block title="Your controls">
                <ul className="mt-1.5 flex flex-col gap-1.5">
                  <Bullet>
                    <strong>Pause</strong> from Settings → Sentry any
                    time. Immediately stops capture.
                  </Bullet>
                  <Bullet>
                    <strong>Purge</strong> wipes every captured event +
                    snapshot on your device and asks the cloud to
                    delete its copy too.
                  </Bullet>
                  <Bullet>
                    A pulsing indicator on the orb shows when Sentry
                    is actively capturing. If you don't see it, it's
                    off.
                  </Bullet>
                </ul>
              </Block>

              <p
                className="text-[11.5px] italic mt-1"
                style={{ color: "rgba(236, 236, 241, 0.55)" }}
              >
                If we ever change what Sentry captures, you'll be asked
                to agree again before the new capture starts.
              </p>
            </div>

            {/* Footer */}
            <div
              className="px-6 py-4 shrink-0 flex items-center justify-between gap-3"
              style={{
                borderTop: "1px solid rgba(255, 255, 255, 0.06)",
                background: "rgba(0, 0, 0, 0.25)",
              }}
            >
              <button
                onClick={onCancel}
                className="text-[12px] font-mono px-3 py-2 rounded-lg transition-colors"
                style={{
                  background: "rgba(255, 255, 255, 0.04)",
                  color: "rgba(236, 236, 241, 0.75)",
                  border: "1px solid rgba(255, 255, 255, 0.10)",
                }}
              >
                Not now
              </button>
              <button
                onClick={handleAgree}
                disabled={!readEnough}
                className="text-[12px] font-mono px-4 py-2 rounded-lg transition-all disabled:cursor-not-allowed"
                style={{
                  background: readEnough
                    ? "linear-gradient(180deg, rgba(189, 158, 255, 0.85), rgba(140, 105, 235, 0.85))"
                    : "rgba(255, 255, 255, 0.06)",
                  color: readEnough
                    ? "rgba(16, 12, 26, 0.95)"
                    : "rgba(236, 236, 241, 0.35)",
                  border: `1px solid ${
                    readEnough ? "rgba(189,158,255,0.55)" : "rgba(255,255,255,0.10)"
                  }`,
                  boxShadow: readEnough
                    ? "0 0 20px -4px rgba(189, 158, 255, 0.55)"
                    : "none",
                  fontWeight: 500,
                }}
                title={
                  readEnough
                    ? "Turn Sentry on and start capturing"
                    : "Scroll to the end to enable"
                }
              >
                I agree — turn Sentry on
              </button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function Block({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section>
      <div
        className="text-[10.5px] uppercase tracking-[0.20em] font-mono"
        style={{ color: "rgba(189, 158, 255, 0.85)" }}
      >
        {title}
      </div>
      <div className="text-[13px] leading-relaxed">{children}</div>
    </section>
  );
}

function Bullet({ children }: { children: React.ReactNode }) {
  return (
    <li className="flex items-start gap-2">
      <span
        className="mt-1.5 w-1.5 h-1.5 rounded-full shrink-0"
        style={{
          background: "rgba(189, 158, 255, 0.85)",
          boxShadow: "0 0 6px rgba(189, 158, 255, 0.65)",
        }}
      />
      <span className="min-w-0 flex-1">{children}</span>
    </li>
  );
}
