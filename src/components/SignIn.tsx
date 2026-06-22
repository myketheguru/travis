/**
 * Sign-in screen — v3 Slice 4 (final). Single sign-in path:
 *
 *   1. Click "Continue from your browser"
 *   2. Desktop opens usetravis.com/app/handoff, loopback-listens
 *   3. Web handles auth on its own (signs the user up via Google
 *      OAuth + cookie session if they're new; uses existing session
 *      if returning)
 *   4. User approves on web → cloud generates a single-use code →
 *      web redirects loopback ?code=… → desktop swaps for JWT
 *
 * Google OAuth in the desktop is GONE. The web is the only place
 * identity is established. Reasons:
 *   - One sign-up funnel (web), not two
 *   - Subscription + onboarding live on web, sign-in shouldn't
 *     bypass that flow
 *   - The handoff itself can do everything Google-direct could,
 *     including first-time sign-up — the web's signed_out branch
 *     just bounces through Google OAuth and lands back at handoff
 */
import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { PresenceOrb } from "./PresenceOrb";
import {
  cloudHandoffFromWeb,
  cloudSignInCancel,
  type CloudUser,
} from "../lib/cloud";

interface Props {
  onSignedIn: (user: CloudUser) => void;
}

export function SignIn({ onSignedIn }: Props) {
  const [status, setStatus] = useState<"idle" | "loading" | "error">("idle");
  const [error, setError] = useState<string | null>(null);

  async function startHandoff() {
    setStatus("loading");
    setError(null);
    try {
      const user = await cloudHandoffFromWeb();
      onSignedIn(user);
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : "Sign-in didn't complete. Please try again.";
      if (msg.toLowerCase().includes("canceled") || msg.toLowerCase().includes("cancelled")) {
        setStatus("idle");
        setError(null);
        return;
      }
      setStatus("error");
      setError(msg);
    }
  }

  async function handleCancel() {
    try {
      await cloudSignInCancel();
    } catch {
      /* idempotent */
    }
  }

  useEffect(() => {
    return () => {
      void cloudSignInCancel().catch(() => {});
    };
  }, []);

  return (
    <div className="min-h-screen flex items-center justify-center px-6">
      <motion.div
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, ease: [0.2, 0.8, 0.2, 1] }}
        className="w-full max-w-md text-center"
      >
        <div className="flex justify-center mb-8">
          <PresenceOrb size={140} />
        </div>

        <h1 className="text-bone text-3xl font-light tracking-tight mb-3">
          Sign in to Travis
        </h1>
        <p className="text-bone-3 text-sm leading-relaxed mb-10 max-w-sm mx-auto">
          Travis lives on the web first. Sign in on usetravis.com once and
          every device you install Travis on will pick up your session
          automatically.
        </p>

        <button
          onClick={startHandoff}
          disabled={status === "loading"}
          className="inline-flex items-center justify-center gap-3 px-5 py-3 rounded-xl bg-bone text-ink font-medium text-sm transition-all hover:bg-white hover:-translate-y-px disabled:opacity-60 disabled:hover:translate-y-0"
        >
          <BrowserIcon />
          <span>
            {status === "loading" ? "Waiting for browser…" : "Continue from your browser"}
          </span>
        </button>

        {status === "loading" && (
          <div className="mt-6 flex flex-col items-center gap-3">
            <p className="text-xs text-bone-3 leading-relaxed max-w-xs mx-auto">
              We've opened the Travis dashboard in your browser. If you don't
              have an account yet, you'll be guided through signup. Then click
              Approve to sign in here.
            </p>
            <button
              onClick={handleCancel}
              className="text-xs text-bone-3 hover:text-bone-2 underline underline-offset-4 decoration-bone-3/40 transition-colors"
            >
              Cancel and try again
            </button>
          </div>
        )}

        {error && (
          <div className="mt-8 px-4 py-3 rounded-lg border border-warn/30 bg-warn/5 text-warn text-xs leading-relaxed">
            {error}
          </div>
        )}

        <p className="mt-12 text-[11px] text-bone-3 leading-relaxed">
          By continuing you agree to our{" "}
          <a
            href="https://usetravis.com/terms"
            target="_blank"
            rel="noopener"
            className="text-pulse-2 underline underline-offset-2 decoration-pulse-2/40 hover:text-bone"
          >
            Terms
          </a>{" "}
          and{" "}
          <a
            href="https://usetravis.com/privacy"
            target="_blank"
            rel="noopener"
            className="text-pulse-2 underline underline-offset-2 decoration-pulse-2/40 hover:text-bone"
          >
            Privacy Policy
          </a>
          .
        </p>
      </motion.div>
    </div>
  );
}

function BrowserIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="M3 9 H21" />
      <circle cx="6" cy="6.5" r="0.6" fill="currentColor" />
      <circle cx="8" cy="6.5" r="0.6" fill="currentColor" />
      <circle cx="10" cy="6.5" r="0.6" fill="currentColor" />
    </svg>
  );
}

