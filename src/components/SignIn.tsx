/**
 * Sign-in screen — Phase 1 of the v2 cloud-first architecture.
 *
 * Shown whenever the desktop has no valid Travis Cloud session. Drives
 * the Google OAuth loopback flow via cloudSignInWithGoogle().
 *
 * Intentionally minimal: this is the single source of truth for sign-in
 * and it sits in front of everything else, including onboarding.
 */
import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { PresenceOrb } from "./PresenceOrb";
import { cloudSignInCancel, cloudSignInWithGoogle, type CloudUser } from "../lib/cloud";

interface Props {
  onSignedIn: (user: CloudUser) => void;
}

export function SignIn({ onSignedIn }: Props) {
  const [status, setStatus] = useState<"idle" | "loading" | "error">("idle");
  const [error, setError] = useState<string | null>(null);

  async function handleSignIn() {
    setStatus("loading");
    setError(null);
    try {
      const user = await cloudSignInWithGoogle();
      onSignedIn(user);
    } catch (e) {
      const msg =
        e instanceof Error
          ? e.message
          : typeof e === "string"
          ? e
          : "Sign-in didn't complete. Please try again.";
      // Surface canceled as a soft state, not an error.
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
      /* idempotent; ignore */
    }
    // The inflight invoke will reject as 'sign-in canceled' and the
    // handler above turns it into status: idle.
  }

  // If the screen unmounts mid-flight (user navigated away somehow),
  // make sure the backend loopback stops blocking on the port.
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
          Sign in with Google so Travis can remember your work across devices
          and keep things running while you're away.
        </p>

        <button
          onClick={handleSignIn}
          disabled={status === "loading"}
          className="inline-flex items-center justify-center gap-3 px-5 py-3 rounded-xl bg-bone text-ink font-medium text-sm transition-all hover:bg-white hover:-translate-y-px disabled:opacity-60 disabled:hover:translate-y-0"
        >
          <GoogleIcon />
          <span>{status === "loading" ? "Waiting for browser…" : "Continue with Google"}</span>
        </button>

        {status === "loading" && (
          <div className="mt-6 flex flex-col items-center gap-3">
            <p className="text-xs text-bone-3 leading-relaxed max-w-xs mx-auto">
              We've opened your browser to Google. Finish signing in there, then
              this window will continue.
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
