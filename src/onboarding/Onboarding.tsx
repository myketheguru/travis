import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { PresenceOrb } from "../components/PresenceOrb";
import { useAppStore } from "../stores/app";
import {
  completeOnboarding,
  type Provider,
} from "../lib/ipc";
import { listPacks, setPackEnabled, type PackInfo } from "../lib/packs";
import { Question, inputClass } from "./Question";
import { VoiceDropdown } from "../components/VoiceDropdown";
import { createWorkspace, type WorkspaceCategory } from "../lib/workspaces";

type Draft = {
  name: string;
  role: string;
  org: string;
  contextBlurb: string;
  communicationStyle: string;
  provider: Provider;
  apiKey: string;
  ollamaUrl: string;
  model: string;
};

const initialDraft: Draft = {
  name: "",
  role: "",
  org: "",
  contextBlurb: "",
  communicationStyle: "",
  // v0.20.8 — every user defaults to Travis Cloud. Bringing your own
  // LLM is a Settings concern, not an onboarding one.
  provider: "travis_cloud",
  apiKey: "",
  ollamaUrl: "http://localhost:11434",
  model: "",
};

// Steps:
// 0 welcome · 1 name · 2 role · 3 org · 4 context (opt) · 5 voice (opt)
// 8 pack picker · 9 workspace (opt) · 10 done
//
// Steps 6 (provider picker) and 7 (api key) are GONE as of v0.20.8 —
// every user defaults to Travis Cloud. The old indices stay reserved
// so we don't have to renumber every transition; next() just jumps
// from 5 to 8 unconditionally now. Advanced users wanting to bring
// their own LLM go through Settings post-onboarding.
const TOTAL_STEPS = 11;

export default function Onboarding({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState(0);
  const [draft, setDraft] = useState<Draft>(initialDraft);
  const [error, setError] = useState<string | null>(null);
  const [packs, setPacks] = useState<PackInfo[] | null>(null);
  // v0.20.8 — onboarding is cloud-only. We no longer probe
  // platform_info, run testProvider, or call submit explicitly — the
  // 5 → 8 transition fires completeOnboarding on its own. Bringing
  // your own LLM is a Settings concern.
  const [savingPacks, setSavingPacks] = useState(false);
  const [extraWorkspaceName, setExtraWorkspaceName] = useState("");
  const [extraWorkspaceCategory, setExtraWorkspaceCategory] =
    useState<WorkspaceCategory>("work");
  const [creatingWorkspace, setCreatingWorkspace] = useState(false);
  const pulse = useAppStore((s) => s.pulse);

  // Load the pack list once we cross into the pack-picker step.
  useEffect(() => {
    if (step === 8 && packs === null) {
      listPacks()
        .then(setPacks)
        .catch((e) => setError(e instanceof Error ? e.message : String(e)));
    }
  }, [step, packs]);

  const update = (patch: Partial<Draft>) => {
    setDraft((d) => ({ ...d, ...patch }));
  };

  // v0.20.8 — onboarding is cloud-only now. Steps 6 (provider) and 7
  // (api key) are not shown to any user, regardless of build. Going
  // forward from 5 jumps to 8; going back from 8 jumps to 5.
  // Bringing your own LLM is a Settings concern, not an onboarding one.
  //
  // completeOnboarding fires at the 5 → 8 boundary so the profile +
  // onboarded flag are persisted before the user reaches optional
  // post-steps (pack picker, workspace). Closing the app at any of
  // those still leaves you onboarded.
  const next = () =>
    setStep((s) => {
      const candidate = Math.min(s + 1, TOTAL_STEPS - 1);
      if (candidate === 6 || candidate === 7) {
        if (draft.provider !== ("travis_cloud" as Provider)) {
          setDraft((d) => ({ ...d, provider: "travis_cloud" as Provider }));
        }
        void completeOnboarding({
          name: draft.name.trim(),
          role: draft.role.trim(),
          org: draft.org.trim(),
          contextBlurb: draft.contextBlurb.trim() || undefined,
          communicationStyle: draft.communicationStyle.trim() || undefined,
          provider: "travis_cloud" as Provider,
        }).catch((e) => {
          console.error("travis_cloud onboarding persist failed", e);
        });
        return 8;
      }
      return candidate;
    });
  const back = () =>
    setStep((s) => {
      const candidate = Math.max(s - 1, 0);
      if (candidate === 6 || candidate === 7) {
        return 5;
      }
      return candidate;
    });

  return (
    <main className="relative h-full w-full overflow-y-auto">
      <div className="sticky top-0 z-10 pt-6 pb-2 flex items-center justify-center gap-1.5 pointer-events-none">
        {/* v0.20.8 — skip the dots for steps 6 and 7 (LLM picker + api
            key) since those steps are no longer shown. Otherwise the
            user sees a 9-step flow but an 11-dot progress bar. */}
        {Array.from({ length: TOTAL_STEPS })
          .map((_, i) => i)
          .filter((i) => i !== 6 && i !== 7)
          .map((i) => (
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

      {step > 0 && step < 10 && (
        <button
          onClick={back}
          className="absolute top-6 left-6 z-20 text-bone-3 hover:text-bone-2 text-xs flex items-center gap-1.5 transition-colors"
        >
          <span aria-hidden>←</span>
          <span>Back</span>
        </button>
      )}

      {/*
        Min-height keeps short-step content vertically centred when the
        window is tall, while overflow-y-auto on <main> takes over when
        the step's content (e.g. the voice picker with 6+ options) is
        taller than the viewport.
      */}
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
              <h1 className="text-5xl font-light tracking-[-0.04em] text-bone">Travis</h1>
              <p className="text-bone-2 text-sm leading-relaxed max-w-sm">
                A thinking and execution layer for your day. A few questions and we're set.
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
              hint="Just your first name is fine."
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
              prompt="And your role?"
              hint="So I can frame work in language that fits."
              canAdvance={draft.role.trim().length > 0}
              onAdvance={next}
            >
              <input
                autoFocus
                value={draft.role}
                onChange={(e) => {
                  pulse();
                  update({ role: e.target.value });
                }}
                placeholder="e.g. Chief Operating Officer"
                className={inputClass}
              />
            </Question>
          )}

          {step === 3 && (
            <Question
              index={3}
              prompt="Where do you work?"
              hint="Travis tailors examples and language to your org."
              canAdvance={draft.org.trim().length > 0}
              onAdvance={next}
            >
              <input
                autoFocus
                value={draft.org}
                onChange={(e) => {
                  pulse();
                  update({ org: e.target.value });
                }}
                placeholder="Organization"
                className={inputClass}
              />
            </Question>
          )}

          {step === 4 && (
            <Question
              index={4}
              prompt="What does your work look like?"
              hint="A short paragraph: what does your org do, who do you serve, and what activities should Travis pay attention to? The more concrete, the more relevant Travis's responses will be. Skip if you'd rather get going."
              canAdvance={true}
              onAdvance={next}
              optional
              onSkip={next}
            >
              <textarea
                autoFocus
                rows={4}
                value={draft.contextBlurb}
                onChange={(e) => {
                  pulse();
                  update({ contextBlurb: e.target.value });
                }}
                placeholder="e.g. We place coaches in NYC public schools and bill the Department of Finance for their hours. I track sessions, sign-off sheets, and invoicing cadence."
                className={
                  inputClass +
                  " resize-none border-b-0 border border-ink-3 focus:border-pulse/70 rounded-md px-3 py-2.5 text-base font-normal leading-relaxed"
                }
              />
            </Question>
          )}

          {step === 5 && (
            <Question
              index={5}
              prompt="How should I sound?"
              hint="Pick the voice that fits how you like to be talked to. You can change it later in Settings."
              canAdvance={true}
              onAdvance={next}
              optional
              onSkip={() => {
                update({ communicationStyle: "" });
                next();
              }}
            >
              <VoiceDropdown
                value={draft.communicationStyle}
                onChange={(v) => update({ communicationStyle: v })}
              />
            </Question>
          )}

          {/* v0.20.8 — steps 6 (provider picker) and 7 (api key) are
              removed. Every user is on Travis Cloud by default; bringing
              your own LLM is a Settings concern, not an onboarding one.
              The step indices stay reserved so the back/next transitions
              don't need renumbering. */}

          {step === 8 && (
            <Question
              index={8}
              prompt="What should Travis help with?"
              hint="Pick the verticals that match your work. You can change these later in Settings → Packs. Pack changes take effect on next launch — you'll need to relaunch Travis once after onboarding for any toggles to apply."
              canAdvance={packs !== null && !savingPacks}
              onAdvance={async () => {
                if (!packs) return;
                setSavingPacks(true);
                setError(null);
                try {
                  // Lock in the user's choice for every pack — even
                  // unchanged defaults — so resolve_enabled_packs has
                  // a definitive answer next launch.
                  for (const p of packs) {
                    await setPackEnabled(p.slug, p.enabled);
                  }
                  setStep(9);
                } catch (e) {
                  setError(e instanceof Error ? e.message : String(e));
                } finally {
                  setSavingPacks(false);
                }
              }}
              advanceLabel={savingPacks ? "Saving…" : "Continue"}
            >
              <div className="flex flex-col gap-2">
                {packs === null && !error && (
                  <p className="text-bone-3 text-xs">Loading packs…</p>
                )}
                {packs && packs.length === 0 && (
                  <p className="text-bone-3 text-xs">
                    No packs are bundled in this build. Travis will run with
                    just the core capabilities (notes, tasks, reminders).
                  </p>
                )}
                {packs?.map((p) => (
                  <label
                    key={p.slug}
                    className={
                      "flex items-start gap-3 rounded-xl border px-4 py-3 transition-all cursor-pointer " +
                      (p.enabled
                        ? "border-pulse/60 bg-pulse/[0.07]"
                        : "border-ink-3 bg-ink-2/30 hover:border-ink-3/80 hover:bg-ink-2/50")
                    }
                  >
                    <input
                      type="checkbox"
                      checked={p.enabled}
                      onChange={(e) => {
                        const enabled = e.target.checked;
                        setPacks((prev) =>
                          prev
                            ? prev.map((x) =>
                                x.slug === p.slug ? { ...x, enabled } : x,
                              )
                            : prev,
                        );
                      }}
                      className="accent-pulse mt-0.5"
                    />
                    <div className="flex-1 min-w-0">
                      <span className="text-bone font-medium">{p.name}</span>
                      {p.description && (
                        <p className="text-bone-3 text-[11px] mt-1 leading-relaxed">
                          {p.description}
                        </p>
                      )}
                    </div>
                  </label>
                ))}
                {error && <p className="text-warn text-xs">{error}</p>}
              </div>
            </Question>
          )}

          {step === 9 && (
            <Question
              index={9}
              prompt="Want a separate workspace for work?"
              hint="Workspaces keep different worlds (work, personal, side projects) from bleeding into each other. Travis can route captures to the right one automatically. You'll start in Personal — add another now or skip and add later in Settings → Workspaces."
              canAdvance={!creatingWorkspace}
              onAdvance={async () => {
                const name = extraWorkspaceName.trim();
                if (!name) {
                  setStep(10);
                  return;
                }
                setCreatingWorkspace(true);
                setError(null);
                try {
                  await createWorkspace({
                    name,
                    category: extraWorkspaceCategory,
                  });
                  setStep(10);
                } catch (e) {
                  setError(e instanceof Error ? e.message : String(e));
                } finally {
                  setCreatingWorkspace(false);
                }
              }}
              advanceLabel={
                creatingWorkspace
                  ? "Adding…"
                  : extraWorkspaceName.trim()
                  ? "Add workspace"
                  : "Continue"
              }
              optional
              onSkip={() => setStep(10)}
            >
              <div className="flex flex-col gap-3">
                <input
                  autoFocus
                  value={extraWorkspaceName}
                  onChange={(e) => setExtraWorkspaceName(e.target.value)}
                  placeholder="e.g. Lead to Empower"
                  className={inputClass}
                />
                <div className="grid grid-cols-3 gap-2">
                  {(
                    [
                      { id: "work" as const, label: "Work" },
                      { id: "personal" as const, label: "Personal" },
                      { id: "other" as const, label: "Other" },
                    ]
                  ).map((c) => {
                    const active = extraWorkspaceCategory === c.id;
                    return (
                      <button
                        key={c.id}
                        type="button"
                        onClick={() => setExtraWorkspaceCategory(c.id)}
                        className={
                          "rounded-xl border px-3 py-2 text-sm transition-all " +
                          (active
                            ? "border-pulse/60 bg-pulse/[0.07] text-bone"
                            : "border-ink-3 bg-ink-2/30 text-bone-2 hover:bg-ink-2/50")
                        }
                      >
                        {c.label}
                      </button>
                    );
                  })}
                </div>
                <p className="text-bone-3 text-[11px] leading-relaxed">
                  Sensitive categories (Health, Therapy, Legal, Finance) are
                  added later from Settings — they default to isolated and
                  deserve a deliberate add.
                </p>
                {error && <p className="text-warn text-xs">{error}</p>}
              </div>
            </Question>
          )}

          {step === 10 && (
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
                <kbd className="px-1.5 py-0.5 rounded border border-ink-3 bg-ink-2/60 text-bone-2 font-mono text-[10px]">Ctrl</kbd>{" "}
                +{" "}
                <kbd className="px-1.5 py-0.5 rounded border border-ink-3 bg-ink-2/60 text-bone-2 font-mono text-[10px]">J</kbd>{" "}
                anywhere to think out loud.
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
