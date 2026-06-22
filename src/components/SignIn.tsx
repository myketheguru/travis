/**
 * Sign-in screen — v3 Slice 4 primary path is web handoff. Google-direct
 * stays as a fallback for users without a browser session yet.
 *
 * Primary CTA: "Continue from your browser" → opens usetravis.com/app/handoff,
 * user approves on web (signing in there first if needed), desktop catches a
 * single-use code on its loopback listener, exchanges for a JWT, signed in.
 *
 * Fallback: the v2 direct Google OAuth still works, kept behind a smaller
 * link for users on a fresh device who haven't been to the web yet.
 */
import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { PresenceOrb } from "./PresenceOrb";
import {
  cloudHandoffFromWeb,
  cloudSignInCancel,
  cloudSignInWithGoogle,
  type CloudUser,
} from "../lib/cloud";

interface Props {
  onSignedIn: (user: CloudUser) => void;
}

type Method = "handoff" | "google";

export function SignIn({ onSignedIn }: Props) {
  const [status, setStatus] = useState<"idle" | "loading" | "error">("idle");
  const [error, setError] = useState<string | null>(null);
  const [method, setMethod] = useState<Method | null>(null);

  function softFail(msg: string) {
    if (msg.toLowerCase().includes("canceled") || msg.toLowerCase().includes("cancelled")) {
      setStatus("idle");
      setError(null);
      return true;
    }
    return false;
  }

  async function startHandoff() {
    setMethod("handoff");
    setStatus("loading");
    setError(null);
    try {
      const user = await cloudHandoffFromWeb();
      onSignedIn(user);
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : "Sign-in didn't complete. Please try again.";
      if (softFail(msg)) return;
      setStatus("error");
      setError(msg);
    }
  }

  async function startGoogleDirect() {
    setMethod("google");
    setStatus("loading");
    setError(null);
    try {
      const user = await cloudSignInWithGoogle();
      onSignedIn(user);
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : "Sign-in didn't complete. Please try again.";
      if (softFail(msg)) return;
      setStatus("error");
      setError(msg);
    }
  }

  async function handleCancel() {
    try {
      await cloudSignInCancel();
    } catch {
      /* idempotent; ignore */
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
          One identity across web and desktop. Travis follows you between
          devices and keeps working while you're away.
        </p>

        <button
          onClick={startHandoff}
          disabled={status === "loading"}
          className="inline-flex items-center justify-center gap-3 px-5 py-3 rounded-xl bg-bone text-ink font-medium text-sm transition-all hover:bg-white hover:-translate-y-px disabled:opacity-60 disabled:hover:translate-y-0"
        >
          <BrowserIcon />
          <span>
            {status === "loading" && method === "handoff"
              ? "Waiting for browser…"
              : "Continue from your browser"}
          </span>
        </button>

        {status === "loading" && method === "handoff" && (
          <div className="mt-6 flex flex-col items-center gap-3">
            <p className="text-xs text-bone-3 leading-relaxed max-w-xs mx-auto">
              We've opened the Travis dashboard in your browser. Sign in there
              (if you haven't already) and click Approve to sign in here.
            </p>
            <button
              onClick={handleCancel}
              className="text-xs text-bone-3 hover:text-bone-2 underline underline-offset-4 decoration-bone-3/40 transition-colors"
            >
              Cancel and try again
            </button>
          </div>
        )}

        <div className="mt-7 flex items-center gap-3 text-bone-3 text-[10px] tracking-wider uppercase">
          <div className="h-px flex-1 bg-bone-3/15" />
          or
          <div className="h-px flex-1 bg-bone-3/15" />
        </div>

        <button
          onClick={startGoogleDirect}
          disabled={status === "loading"}
          className="mt-7 inline-flex items-center justify-center gap-3 px-4 py-2.5 rounded-lg border border-bone-3/20 text-bone-2 text-sm hover:border-bone-3/40 hover:text-bone disabled:opacity-50 transition-colors"
        >
          <GoogleIcon />
          <span>
            {status === "loading" && method === "google"
              ? "Waiting for Google…"
              : "Sign in with Google directly"}
          </span>
        </button>

        {status === "loading" && method === "google" && (
          <div className="mt-4 flex flex-col items-center gap-2">
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
          New here?{" "}
          <a
            href="https://usetravis.com/app/"
            target="_blank"
            rel="noopener"
            className="text-pulse-2 underline underline-offset-2 decoration-pulse-2/40 hover:text-bone"
          >
            Start on the web
          </a>{" "}
          — sign up, pick a plan, then come back here.
        </p>
        <p className="mt-3 text-[11px] text-bone-3 leading-relaxed">
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

function GoogleIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 18 18" aria-hidden="true">
      <path
        fill="#4285F4"
        d="M17.64 9.2c0-.64-.06-1.25-.16-1.84H9v3.48h4.84a4.14 4.14 0 0 1-1.8 2.72v2.26h2.92c1.7-1.57 2.68-3.88 2.68-6.62z"
      />
      <path
        fill="#34A853"
        d="M9 18c2.43 0 4.47-.8 5.96-2.18l-2.92-2.26c-.8.54-1.83.86-3.04.86-2.34 0-4.32-1.58-5.03-3.7H.96v2.32A9 9 0 0 0 9 18z"
      />
      <path
        fill="#FBBC05"
        d="M3.97 10.72A5.4 5.4 0 0 1 3.68 9c0-.6.1-1.18.29-1.72V4.96H.96A9 9 0 0 0 0 9c0 1.45.35 2.83.96 4.04l3.01-2.32z"
      />
      <path
        fill="#EA4335"
        d="M9 3.58c1.32 0 2.5.45 3.44 1.35l2.58-2.58A9 9 0 0 0 9 0 9 9 0 0 0 .96 4.96l3.01 2.32C4.68 5.16 6.66 3.58 9 3.58z"
      />
    </svg>
  );
}
