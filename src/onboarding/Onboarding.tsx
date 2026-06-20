/**
 * v2 Phase 1.5 — identity + preferences only onboarding.
 *
 * The user has already signed in via the cloud gate above; we know
 * their email and (usually) their name from the Google profile. This
 * flow confirms what we'll call them and how Travis should sound.
 *
 * Travis never asks "what do you do" — users juggle multiple
 * companies / startups / clients, and pre-scoping them to one would be
 * wrong. Context comes from use, not from interrogation.
 */
import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { PresenceOrb } from "../components/PresenceOrb";
import { useAppStore } from "../stores/app";
import { completeOnboarding, type Provider } from "../lib/ipc";
import { cloudStatus } from "../lib/cloud";
import { Question, inputClass } from "./Question";
import { VoiceDropdown } from "../components/VoiceDropdown";

type Draft = {
  name: string;
  communicationStyle: string;
};

// Identity + preferences only. Welcome → confirm name → voice → done.
const TOTAL_STEPS = 4;

export default function Onboarding({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState(0);
  const [draft, setDraft] = useState<Draft>({ name: "", communicationStyle: "" });
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pulse = useAppStore((s) => s.pulse);

  // Pre-fill the display name from the cloud user profile (Google
  // gives us this on sign-in). If anything fails we just start blank
  // — the user types their name on step 1.
  useEffect(() => {
    cloudStatus()
      .then((s) => {
        if (s.signedIn && s.user?.name) {
          setDraft((d) => ({ ...d, name: s.user!.name! }));
        }
      })
      .catch(() => {
        // Cloud unreachable here would be unusual (we just came
        // through the sign-in gate) — fail open with a blank name.
      });
  }, []);

  const update = (patch: Partial<Draft>) => setDraft((d) => ({ ...d, ...patch }));

  const next = () => setStep((s) => Math.min(s + 1, TOTAL_STEPS - 1));
  const back = () => setStep((s) => Math.max(s - 1, 0));

  // Persist the profile when the user leaves the voice step. We send
  // empty strings for role + org — those fields stay on the schema
  // for now, but Travis is no longer scoped by them. They'll get
  // dropped from the DB schema in Phase 2.
  async function persistAndFinish() {
    if (submitting) return;
    setSubmitting(true);
    setError(null);
    try {
      await completeOnboarding({
        name: draft.name.trim(),
        role: "",
        org: "",
        contextBlurb: undefined,
        communicationStyle: draft.communicationStyle.trim() || undefined,
        provider: "travis_cloud" as Provider,
      });
      next();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <main className="relative h-full w-full overflow-y-auto">
      <div className="sticky top-0 z-10 pt-6 pb-2 flex items-center justify-center gap-1.5 pointer-events-none">
        {Array.from({ length: TOTAL_STEPS }).map((_, i) => (
          <motion.div
            key={i}
            className="h-[3px] rounded-full"
            initial={false}
            animate={{
              width: i === step ? 22 : 6,
              backgroundColor:
                i < step
                  ? "rgba(110,196,232,0.75)"
                  : i === step
                  ? "rgba(168,124,232,0.95)"
                  : "rgba(108,108,124,0.30)",
            }}
            transition={{ duration: 0.4, ease: "easeOut" }}
          />
        ))}
      </div>

      {step > 0 && step < TOTAL_STEPS - 1 && (
        <button
          onClick={back}
          className="absolute top-6 left-6 z-20 text-bone-3 hover:text-bone-2 text-xs flex items-center gap-1.5 transition-colors"
        >
          <span aria-hidden>←</span>
          <span>Back</span>
        </button>
      )}

      <div className="min-h-[calc(100%-32px)] flex flex-col items-center justify-center px-10 pt-4 pb-12">
        <div className="flex flex-col items-center gap-7 w-full max-w-xl">
          <PresenceOrb size={96} />

          <div className="relative z-10 w-full">
            <AnimatePresence mode="wait">
              {step === 0 && (
                <motion.div
                  key="welcome"
                  initial={{ opacity: 0, y: 24 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -24 }}
                  transition={{ duration: 0.5, ease: [0.16, 1, 0.3, 1] }}
                  className="flex flex-col items-center text-center gap-5"
                >
                  <h1 className="text-5xl font-light tracking-[-0.04em] text-bone">
                    {draft.name ? `Welcome, ${draft.name.split(" ")[0]}.` : "Welcome."}
                  </h1>
                  <p className="text-bone-2 text-sm leading-relaxed max-w-sm">
                    Travis is your thinking and execution layer for the day.
                    Two quick things, then we begin.
                  </p>
                  <button
                    onClick={next}
                    className="mt-3 px-6 py-2.5 rounded-full bg-bone/95 text-ink text-sm font-medium hover:bg-bone transition-colors"
                  >
                    Begin
                  </button>
                </motion.div>
              )}

              {step === 1 && (
                <Question
                  index={1}
                  prompt="What should I call you?"
                  hint="Just your first name is fine. We pulled this from your Google account — adjust if you'd like something else."
                  canAdvance={draft.name.trim().length > 0}
                  onAdvance={next}
                >
                  <input
                    autoFocus
                    value={draft.name}
                    onChange={(e) => {
                      pulse();
                      update({ name: e.target.value });
                    }}
                    placeholder="Your name"
                    className={inputClass}
                  />
                </Question>
              )}

              {step === 2 && (
                <Question
                  index={2}
                  prompt="How should I sound?"
                  hint="Pick the voice that fits how you like to be talked to. You can change it later in Settings."
                  canAdvance={!submitting}
                  onAdvance={persistAndFinish}
                  advanceLabel={submitting ? "Saving…" : "Continue"}
                  optional
                  onSkip={() => {
                    update({ communicationStyle: "" });
                    persistAndFinish();
                  }}
                >
                  <VoiceDropdown
                    value={draft.communicationStyle}
                    onChange={(v) => update({ communicationStyle: v })}
                  />
                  {error && <p className="text-warn text-xs mt-3">{error}</p>}
                </Question>
              )}

              {step === 3 && (
                <motion.div
                  key="done"
                  initial={{ opacity: 0, y: 24 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -24 }}
                  transition={{ duration: 0.6, ease: [0.16, 1, 0.3, 1] }}
                  className="flex flex-col items-center text-center gap-5"
                >
                  <h2 className="text-3xl font-light tracking-tight text-bone">
                    Hello, {draft.name.split(" ")[0] || draft.name}.
                  </h2>
                  <p className="text-bone-2 text-sm max-w-sm leading-relaxed">
                    I'm here. Press{" "}
                    <kbd className="px-1.5 py-0.5 rounded border border-ink-3 bg-ink-2/60 text-bone-2 font-mono text-[10px]">
                      Ctrl
                    </kbd>{" "}
                    +{" "}
                    <kbd className="px-1.5 py-0.5 rounded border border-ink-3 bg-ink-2/60 text-bone-2 font-mono text-[10px]">
                      J
                    </kbd>{" "}
                    anywhere to think out loud. I'll learn the rest by working
                    with you.
                  </p>
                  <button
                    onClick={onDone}
                    className="mt-3 px-6 py-2.5 rounded-full bg-bone/95 text-ink text-sm font-medium hover:bg-bone transition-colors"
                  >
                    Enter
                  </button>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        </div>
      </div>
    </main>
  );
}
