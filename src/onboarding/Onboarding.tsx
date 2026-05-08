import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { PresenceOrb } from "../components/PresenceOrb";
import { useAppStore } from "../stores/app";
import {
  completeOnboarding,
  testProvider,
  type OnboardingPayload,
  type PingResult,
  type Provider,
} from "../lib/ipc";
import { listPacks, setPackEnabled, type PackInfo } from "../lib/packs";
import { Question, inputClass } from "./Question";
import { VoiceDropdown } from "../components/VoiceDropdown";

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
  provider: "claude",
  apiKey: "",
  ollamaUrl: "http://localhost:11434",
  model: "",
};

// Steps:
// 0 welcome · 1 name · 2 role · 3 org · 4 context (opt) · 5 voice (opt)
// 6 provider · 7 api key · 8 pack picker · 9 done
const TOTAL_STEPS = 10;

const providers: { id: Provider; name: string; blurb: string; needsKey: boolean }[] = [
  { id: "claude", name: "Claude",  blurb: "Anthropic — best reasoning, prompt caching",  needsKey: true },
  { id: "openai", name: "OpenAI",  blurb: "GPT-class models, broadly compatible",         needsKey: true },
  { id: "ollama", name: "Ollama",  blurb: "Run locally, private, no API key",             needsKey: false },
];

const defaultModels: Record<Provider, string> = {
  claude: "claude-sonnet-4-6",
  openai: "gpt-4o",
  ollama: "llama3.1:8b",
};

export default function Onboarding({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState(0);
  const [draft, setDraft] = useState<Draft>(initialDraft);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<PingResult | null>(null);
  const [packs, setPacks] = useState<PackInfo[] | null>(null);
  const [savingPacks, setSavingPacks] = useState(false);
  const setActivity = useAppStore((s) => s.setActivity);
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
    setTestResult(null);
    setDraft((d) => ({ ...d, ...patch }));
  };

  const runTest = async () => {
    setTesting(true);
    setTestResult(null);
    setActivity("thinking");
    try {
      const r = await testProvider({
        provider: draft.provider,
        apiKey: draft.provider === "ollama" ? undefined : draft.apiKey || undefined,
        ollamaUrl: draft.provider === "ollama" ? draft.ollamaUrl : undefined,
        model: draft.model || undefined,
      });
      setTestResult(r);
    } catch (e) {
      setTestResult({
        ok: false,
        model: draft.model || defaultModels[draft.provider],
        latencyMs: 0,
        message: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setActivity("idle");
      setTesting(false);
    }
  };

  const provider = providers.find((p) => p.id === draft.provider)!;

  const submit = async () => {
    setError(null);
    setSubmitting(true);
    setActivity("thinking");
    try {
      const payload: OnboardingPayload = {
        name: draft.name.trim(),
        role: draft.role.trim(),
        org: draft.org.trim(),
        contextBlurb: draft.contextBlurb.trim() || undefined,
        communicationStyle: draft.communicationStyle.trim() || undefined,
        provider: draft.provider,
        apiKey: draft.provider === "ollama" ? undefined : draft.apiKey || undefined,
        ollamaUrl: draft.provider === "ollama" ? draft.ollamaUrl : undefined,
        model: draft.model || undefined,
      };
      await completeOnboarding(payload);
      setStep(8);
      setActivity("idle");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setActivity("idle");
    } finally {
      setSubmitting(false);
    }
  };

  const next = () => setStep((s) => Math.min(s + 1, TOTAL_STEPS - 1));
  const back = () => setStep((s) => Math.max(s - 1, 0));

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

      {step > 0 && step < 9 && (
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

          {step === 6 && (
            <Question
              index={6}
              prompt="Which mind should I think with?"
              hint="You can switch this any time in settings."
              canAdvance={true}
              onAdvance={next}
            >
              <div className="flex flex-col gap-2">
                {providers.map((p) => {
                  const active = draft.provider === p.id;
                  return (
                    <button
                      key={p.id}
                      onClick={() => update({ provider: p.id })}
                      className={
                        "text-left rounded-xl border px-4 py-3 transition-all " +
                        (active
                          ? "border-pulse/60 bg-pulse/[0.07]"
                          : "border-ink-3 bg-ink-2/30 hover:border-ink-3/80 hover:bg-ink-2/50")
                      }
                    >
                      <div className="flex items-center justify-between">
                        <span className="text-bone font-medium">{p.name}</span>
                        {active && (
                          <span className="h-1.5 w-1.5 rounded-full bg-pulse-2 shadow-[0_0_8px_rgba(110,196,232,0.7)]" />
                        )}
                      </div>
                      <p className="text-bone-3 text-xs mt-0.5">{p.blurb}</p>
                    </button>
                  );
                })}
              </div>
            </Question>
          )}

          {step === 7 && (
            <Question
              index={7}
              prompt={
                provider.needsKey
                  ? `Drop your ${provider.name} key.`
                  : "Where's your Ollama running?"
              }
              hint={
                provider.needsKey
                  ? "Stored in your OS keychain — never written to disk in plain text."
                  : "The default works for most local installs."
              }
              canAdvance={
                provider.needsKey
                  ? draft.apiKey.trim().length > 0
                  : draft.ollamaUrl.trim().length > 0
              }
              onAdvance={submit}
              advanceLabel={submitting ? "Setting up…" : "Finish"}
            >
              <div className="flex flex-col gap-5">
                {provider.needsKey ? (
                  <input
                    autoFocus
                    type="password"
                    value={draft.apiKey}
                    onChange={(e) => {
                      pulse();
                      update({ apiKey: e.target.value });
                    }}
                    placeholder={draft.provider === "claude" ? "sk-ant-..." : "sk-..."}
                    className={inputClass + " font-mono text-base"}
                  />
                ) : (
                  <input
                    autoFocus
                    value={draft.ollamaUrl}
                    onChange={(e) => {
                      pulse();
                      update({ ollamaUrl: e.target.value });
                    }}
                    className={inputClass + " font-mono text-base"}
                  />
                )}

                <details className="group">
                  <summary className="text-bone-3 text-xs cursor-pointer hover:text-bone-2 transition-colors list-none flex items-center gap-2">
                    <span className="text-pulse-2/70 transition-transform group-open:rotate-90">›</span>
                    <span>Specific model? (optional, default {defaultModels[draft.provider]})</span>
                  </summary>
                  <div className="mt-3 ml-4">
                    <input
                      value={draft.model}
                      onChange={(e) => {
                        pulse();
                        update({ model: e.target.value });
                      }}
                      placeholder={defaultModels[draft.provider]}
                      className={inputClass + " font-mono text-base"}
                    />
                  </div>
                </details>

                <div className="flex items-center gap-3 text-xs">
                  <button
                    type="button"
                    onClick={runTest}
                    disabled={
                      testing ||
                      (provider.needsKey
                        ? draft.apiKey.trim().length === 0
                        : draft.ollamaUrl.trim().length === 0)
                    }
                    className="text-pulse-2 hover:text-bone disabled:opacity-30 disabled:cursor-not-allowed underline-offset-4 hover:underline transition-colors"
                  >
                    {testing ? "Testing…" : "Test connection"}
                  </button>
                  {testResult && (
                    <span
                      className={
                        "flex items-center gap-1.5 " +
                        (testResult.ok ? "text-pulse-2" : "text-warn")
                      }
                    >
                      <span
                        className={
                          "h-1.5 w-1.5 rounded-full " +
                          (testResult.ok ? "bg-pulse-2" : "bg-warn")
                        }
                      />
                      {testResult.ok
                        ? `connected · ${testResult.model} · ${testResult.latencyMs}ms`
                        : testResult.message ?? "failed"}
                    </span>
                  )}
                </div>

                {error && <p className="text-warn text-xs">{error}</p>}
              </div>
            </Question>
          )}

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
