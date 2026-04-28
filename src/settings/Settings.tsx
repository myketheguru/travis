import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import {
  getProactiveConfig,
  getShellEnabled,
  getUserProfile,
  hasApiKey,
  setApiKey,
  setProactiveEnabled,
  setShellEnabled,
  testProvider,
  updateProfile,
  type PingResult,
  type ProactiveConfig,
  type Provider,
  type UserProfile,
} from "../lib/ipc";
import { checkForUpdate, installUpdate, type UpdateInfo } from "../lib/updater";
import { VOICE_PRESETS, presetFromDescription } from "../onboarding/voicePresets";
import { useAppStore } from "../stores/app";

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

export default function Settings({ onClose }: { onClose: () => void }) {
  const [draft, setDraft] = useState<Draft | null>(null);
  const [keyExists, setKeyExists] = useState(false);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [test, setTest] = useState<PingResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [savedHint, setSavedHint] = useState<string | null>(null);
  const setActivity = useAppStore((s) => s.setActivity);
  const setProfile = useAppStore((s) => s.setProfile);

  useEffect(() => {
    (async () => {
      const p = await getUserProfile();
      if (!p) return;
      setDraft({
        name: p.name,
        role: p.role,
        org: p.org,
        contextBlurb: p.contextBlurb ?? "",
        communicationStyle: p.communicationStyle ?? "",
        provider: p.llmProvider,
        apiKey: "",
        ollamaUrl: p.ollamaUrl ?? "http://localhost:11434",
        model: p.model ?? "",
      });
      if (p.llmProvider !== "ollama") {
        setKeyExists(await hasApiKey(p.llmProvider));
      }
    })();
  }, []);

  const update = (patch: Partial<Draft>) => {
    setDraft((d) => (d ? { ...d, ...patch } : d));
    setTest(null);
    setSavedHint(null);
  };

  const runTest = async () => {
    if (!draft) return;
    setTesting(true);
    setTest(null);
    setError(null);
    setActivity("thinking");
    try {
      const r = await testProvider({
        provider: draft.provider,
        apiKey: draft.provider === "ollama" ? undefined : draft.apiKey || undefined,
        ollamaUrl: draft.provider === "ollama" ? draft.ollamaUrl : undefined,
        model: draft.model || undefined,
      });
      setTest(r);
    } catch (e) {
      setTest({
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

  const save = async () => {
    if (!draft || saving) return;
    setSaving(true);
    setError(null);
    setSavedHint(null);
    try {
      await updateProfile({
        name: draft.name.trim(),
        role: draft.role.trim(),
        org: draft.org.trim(),
        contextBlurb: draft.contextBlurb.trim() || undefined,
        communicationStyle: draft.communicationStyle.trim() || undefined,
        provider: draft.provider,
        apiKey:
          draft.provider === "ollama"
            ? undefined
            : draft.apiKey.trim() || undefined,
        ollamaUrl: draft.provider === "ollama" ? draft.ollamaUrl : undefined,
        model: draft.model || undefined,
      });

      if (draft.provider !== "ollama" && draft.apiKey.trim()) {
        try {
          await setApiKey(draft.provider, draft.apiKey.trim());
        } catch (e) {
          setError(`profile saved but key store failed: ${e}`);
          return;
        }
      }

      const refreshed = (await getUserProfile()) as UserProfile;
      setProfile(refreshed);
      setKeyExists(
        draft.provider === "ollama" ? false : await hasApiKey(draft.provider),
      );
      setDraft({ ...draft, apiKey: "" });
      setSavedHint("Saved.");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  if (!draft) {
    return <main className="h-full w-full" />;
  }

  const provider = providers.find((p) => p.id === draft.provider)!;

  return (
    <main className="relative h-full w-full overflow-y-auto">
      <button
        onClick={onClose}
        className="absolute top-6 left-6 text-bone-3 hover:text-bone-2 text-xs flex items-center gap-1.5 transition-colors"
      >
        <span aria-hidden>←</span>
        <span>Back</span>
      </button>

      <motion.div
        className="max-w-xl mx-auto px-10 pt-20 pb-16 flex flex-col gap-7"
        initial={{ opacity: 0, y: 12 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
      >
        <div>
          <h1 className="text-3xl font-light tracking-tight text-bone">Settings</h1>
          <p className="text-bone-3 text-xs mt-1.5">
            Provider, model, and identity. API keys live in your OS keychain.
          </p>
        </div>

        <Section title="Identity">
          <Field label="Name">
            <Input
              value={draft.name}
              onChange={(v) => update({ name: v })}
            />
          </Field>
          <Field label="Role">
            <Input
              value={draft.role}
              onChange={(v) => update({ role: v })}
            />
          </Field>
          <Field label="Organization">
            <Input
              value={draft.org}
              onChange={(v) => update({ org: v })}
            />
          </Field>
        </Section>

        <Section title="Context for Travis">
          <p className="text-bone-3 text-[11px] leading-relaxed -mt-2">
            What you tell Travis here is woven into every system prompt so its
            replies stay grounded in your work — not generic.
          </p>
          <Field label="What your work looks like">
            <TextArea
              value={draft.contextBlurb}
              onChange={(v) => update({ contextBlurb: v })}
              placeholder="A short paragraph: what your org does, who you serve, key activities Travis should pay attention to."
              rows={4}
            />
          </Field>
          <Field label="Voice (optional)">
            <VoicePicker
              value={draft.communicationStyle}
              onChange={(v) => update({ communicationStyle: v })}
            />
          </Field>
        </Section>

        <Section title="Model">
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

          {provider.needsKey ? (
            <Field
              label={
                keyExists
                  ? `${provider.name} API key  (replace existing)`
                  : `${provider.name} API key`
              }
            >
              <Input
                type="password"
                value={draft.apiKey}
                onChange={(v) => update({ apiKey: v })}
                placeholder={
                  keyExists
                    ? "•••••••• stored — paste a new value to replace"
                    : draft.provider === "claude"
                    ? "sk-ant-..."
                    : "sk-..."
                }
                mono
              />
              <p className="text-bone-3 text-[10px] mt-1.5">
                {keyExists
                  ? "Leave blank to keep the stored key. Paste a new value to overwrite."
                  : "Stored in your OS keychain."}
              </p>
            </Field>
          ) : (
            <Field label="Ollama endpoint">
              <Input
                value={draft.ollamaUrl}
                onChange={(v) => update({ ollamaUrl: v })}
                mono
              />
            </Field>
          )}

          <Field label="Model (optional)">
            <Input
              value={draft.model}
              onChange={(v) => update({ model: v })}
              placeholder={defaultModels[draft.provider]}
              mono
            />
          </Field>

          <div className="flex items-center gap-3 mt-1">
            <button
              type="button"
              onClick={runTest}
              disabled={
                testing ||
                (provider.needsKey
                  ? !keyExists && draft.apiKey.trim().length === 0
                  : draft.ollamaUrl.trim().length === 0)
              }
              className="text-pulse-2 hover:text-bone disabled:opacity-30 disabled:cursor-not-allowed text-xs underline-offset-4 hover:underline transition-colors"
            >
              {testing ? "Testing…" : "Test connection"}
            </button>
            {test && (
              <span
                className={
                  "flex items-center gap-1.5 text-xs " +
                  (test.ok ? "text-pulse-2" : "text-warn")
                }
              >
                <span
                  className={
                    "h-1.5 w-1.5 rounded-full " +
                    (test.ok ? "bg-pulse-2" : "bg-warn")
                  }
                />
                {test.ok
                  ? `${test.model} · ${test.latencyMs}ms`
                  : test.message ?? "failed"}
              </span>
            )}
          </div>
        </Section>

        <div className="flex items-center gap-4 pt-2">
          <button
            onClick={save}
            disabled={saving}
            className="px-5 py-2.5 rounded-full bg-bone/95 text-ink text-sm font-medium disabled:opacity-30 hover:bg-bone transition-all min-w-[110px]"
          >
            {saving ? "Saving…" : "Save"}
          </button>
          {savedHint && <span className="text-pulse-2 text-xs">{savedHint}</span>}
          {error && <span className="text-warn text-xs">{error}</span>}
        </div>

        <CalendarSection />
        <ProactiveSection />
        <UpdatesSection />
        <DiagnosticsSection />
        <ShellToolSection />
      </motion.div>
    </main>
  );
}

type ConnectionState = {
  connected: boolean;
  configured: boolean;
  accountId: string | null;
  connectedAt: string | null;
  scopes: string[];
};

function CalendarSection() {
  return (
    <Section title="Connections">
      <p className="text-bone-3 text-[11px] leading-relaxed -mt-2">
        Connect a Google or Microsoft account so Travis can see your calendar
        ("what's on tomorrow?") and send email on your behalf — only when you
        confirm a preview card. Calendar access is read-only.
      </p>
      <ProviderRow
        label="Google"
        description="Calendar (read) · Gmail (send)"
        envHint="TRAVIS_GOOGLE_CLIENT_ID / _SECRET"
        loadStatus={async () => {
          const { calendarStatus } = await import("../lib/calendar");
          return calendarStatus();
        }}
        connectFn={async () => {
          const { calendarConnectGoogle } = await import("../lib/calendar");
          return calendarConnectGoogle();
        }}
        disconnectFn={async () => {
          const { calendarDisconnectGoogle } = await import("../lib/calendar");
          await calendarDisconnectGoogle();
        }}
      />
      <ProviderRow
        label="Microsoft"
        description="Calendar (read) · Outlook (send)"
        envHint="TRAVIS_MICROSOFT_CLIENT_ID / _SECRET"
        loadStatus={async () => {
          const { microsoftStatus } = await import("../lib/calendar");
          return microsoftStatus();
        }}
        connectFn={async () => {
          const { microsoftConnect } = await import("../lib/calendar");
          return microsoftConnect();
        }}
        disconnectFn={async () => {
          const { microsoftDisconnect } = await import("../lib/calendar");
          await microsoftDisconnect();
        }}
      />
    </Section>
  );
}

function ProviderRow({
  label,
  description,
  envHint,
  loadStatus,
  connectFn,
  disconnectFn,
}: {
  label: string;
  description: string;
  envHint: string;
  loadStatus: () => Promise<ConnectionState>;
  connectFn: () => Promise<string>;
  disconnectFn: () => Promise<void>;
}) {
  const [status, setStatus] = useState<ConnectionState | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [hint, setHint] = useState<string | null>(null);

  const refresh = async () => {
    try {
      const s = await loadStatus();
      setStatus(s);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  };

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const connect = async () => {
    setBusy(true);
    setErr(null);
    setHint(`A new browser tab will open. Sign in with ${label} and grant access.`);
    try {
      const email = await connectFn();
      setHint(`Connected as ${email}`);
      await refresh();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
      setHint(null);
    } finally {
      setBusy(false);
    }
  };

  const disconnect = async () => {
    setBusy(true);
    setErr(null);
    try {
      await disconnectFn();
      setHint(null);
      await refresh();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="rounded-xl border border-ink-3 bg-ink-2/30 p-3.5">
      <div className="flex items-baseline justify-between mb-1">
        <span className="text-bone text-sm font-medium">{label}</span>
        <span className="text-bone-3 text-[10px] tracking-wider">{description}</span>
      </div>

      {!status ? (
        <p className="text-bone-3 text-[11px]">Loading…</p>
      ) : !status.configured ? (
        <p className="text-warn text-[11px]">
          Not configured in this build — needs {envHint} at build time.
        </p>
      ) : (
        <div className="flex items-center gap-3 mt-2">
          {status.connected ? (
            <>
              <div className="flex items-center gap-2 text-pulse-2 text-xs">
                <span className="h-1.5 w-1.5 rounded-full bg-pulse-2 shadow-[0_0_8px_rgba(110,196,232,0.6)]" />
                <span>Connected{status.accountId ? ` as ${status.accountId}` : ""}</span>
              </div>
              <button
                onClick={disconnect}
                disabled={busy}
                className="ml-auto text-bone-3 hover:text-warn text-[11px] underline-offset-4 hover:underline disabled:opacity-30"
              >
                disconnect
              </button>
            </>
          ) : (
            <button
              onClick={connect}
              disabled={busy}
              className="px-3.5 py-1.5 rounded-full bg-bone/95 text-ink text-[11px] font-medium disabled:opacity-30 hover:bg-bone transition-colors"
            >
              {busy ? "Opening browser…" : `Connect ${label}`}
            </button>
          )}
        </div>
      )}
      {hint && <p className="text-pulse-2 text-[11px] mt-2">{hint}</p>}
      {err && <p className="text-warn text-[11px] mt-2">{err}</p>}
    </div>
  );
}

function ShellToolSection() {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    getShellEnabled()
      .then(setEnabled)
      .catch(() => setEnabled(false));
  }, []);

  const toggle = async (v: boolean) => {
    setBusy(true);
    setErr(null);
    try {
      await setShellEnabled(v);
      setEnabled(v);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Section title="Computer access">
      <div className="rounded-lg border border-warn/25 bg-warn/[0.04] p-3">
        <div className="text-warn text-[10px] tracking-[0.18em] uppercase mb-1.5">
          Use with care
        </div>
        <p className="text-bone-2 text-[11px] leading-relaxed">
          When on, Travis can ask you for permission to do things on your
          computer — like checking what's in a folder, looking up the version
          of a tool, or running a quick status check. You'll always see exactly
          what it wants to do and click Allow before anything happens.
          Travis won't propose anything destructive, and there's a built-in
          safety filter that refuses dangerous actions even if you say yes.
          Default OFF.
        </p>
      </div>

      <label className="flex items-center gap-2 cursor-pointer select-none">
        <input
          type="checkbox"
          checked={enabled ?? false}
          onChange={(e) => toggle(e.target.checked)}
          disabled={enabled === null || busy}
          className="accent-pulse"
        />
        <span className="text-bone-2 text-sm">
          Let Travis run things on my computer (with my permission)
        </span>
      </label>
      {err && <p className="text-warn text-xs">{err}</p>}
    </Section>
  );
}

function ProactiveSection() {
  const [cfg, setCfg] = useState<ProactiveConfig | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    getProactiveConfig()
      .then(setCfg)
      .catch(() => setCfg({ enabled: false, lastAt: null }));
  }, []);

  const toggle = async (v: boolean) => {
    setBusy(true);
    setErr(null);
    try {
      await setProactiveEnabled(v);
      setCfg((c) => (c ? { ...c, enabled: v } : { enabled: v, lastAt: null }));
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const lastAtPretty = (() => {
    if (!cfg?.lastAt) return null;
    try {
      const d = new Date(cfg.lastAt);
      return d.toLocaleString();
    } catch {
      return cfg.lastAt;
    }
  })();

  return (
    <Section title="Proactive nudges">
      <p className="text-bone-3 text-[11px] leading-relaxed -mt-2">
        Travis quietly checks in roughly every few hours during waking hours
        (8am–10pm) — but only when there's something specific worth surfacing,
        like an overdue task, a thread waiting on you, or a capability gap.
        Stays silent otherwise. You'll get a notification and a card in the
        Asks of me thread. On by default — turn off if it's too chatty.
      </p>

      <label className="flex items-center gap-2 cursor-pointer select-none">
        <input
          type="checkbox"
          checked={cfg?.enabled ?? false}
          onChange={(e) => toggle(e.target.checked)}
          disabled={cfg === null || busy}
          className="accent-pulse"
        />
        <span className="text-bone-2 text-sm">
          Let Travis nudge me when something's worth surfacing
        </span>
      </label>

      {lastAtPretty && (
        <p className="text-bone-3 text-[10px]">Last nudge: {lastAtPretty}</p>
      )}
      {err && <p className="text-warn text-xs">{err}</p>}
    </Section>
  );
}

function UpdatesSection() {
  const status = useAppStore((s) => s.status);
  const [available, setAvailable] = useState<UpdateInfo | null>(null);
  const [busy, setBusy] = useState<"check" | "install" | null>(null);
  const [hint, setHint] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const check = async () => {
    setBusy("check");
    setHint(null);
    setErr(null);
    try {
      const r = await checkForUpdate();
      setAvailable(r);
      if (!r) setHint("You're on the latest version.");
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const install = async () => {
    setBusy("install");
    setErr(null);
    setHint("Downloading and installing… the app will restart.");
    try {
      await installUpdate();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
      setHint(null);
    } finally {
      setBusy(null);
    }
  };

  return (
    <Section title="Updates">
      <div className="flex items-center gap-3">
        <span className="text-bone-3 text-[11px] font-mono">
          v{status?.version ?? "?"}
        </span>
        {available ? (
          <button
            onClick={install}
            disabled={busy !== null}
            className="px-4 py-1.5 rounded-full bg-pulse-2/15 border border-pulse-2/40 text-pulse-2 text-[11px] font-medium hover:bg-pulse-2/25 disabled:opacity-30 transition-colors"
          >
            {busy === "install" ? "Installing…" : `Install v${available.version}`}
          </button>
        ) : (
          <button
            onClick={check}
            disabled={busy !== null}
            className="px-4 py-1.5 rounded-full bg-bone/95 text-ink text-[11px] font-medium disabled:opacity-30 hover:bg-bone transition-colors"
          >
            {busy === "check" ? "Checking…" : "Check for updates"}
          </button>
        )}
      </div>
      {available?.notes && (
        <pre className="text-bone-3 text-[11px] whitespace-pre-wrap font-sans leading-relaxed">
          {available.notes}
        </pre>
      )}
      {hint && <p className="text-pulse-2 text-[11px]">{hint}</p>}
      {err && <p className="text-warn text-[11px]">{err}</p>}
    </Section>
  );
}

function DiagnosticsSection() {
  const showDiagnostics = useAppStore((s) => s.showDiagnostics);
  const setShowDiagnostics = useAppStore((s) => s.setShowDiagnostics);
  return (
    <Section title="Advanced">
      <p className="text-bone-3 text-[11px] leading-relaxed -mt-2">
        Reveal extra Manage tabs (Entities, Summaries, Asks of me). These are
        internal views into what Travis has captured — useful for debugging,
        not needed day-to-day.
      </p>
      <label className="flex items-center gap-2 cursor-pointer select-none">
        <input
          type="checkbox"
          checked={showDiagnostics}
          onChange={(e) => setShowDiagnostics(e.target.checked)}
          className="accent-pulse"
        />
        <span className="text-bone-2 text-sm">Show diagnostics tabs in Manage</span>
      </label>
    </Section>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-bone-3 text-[11px] tracking-[0.2em] uppercase">{title}</h2>
      {children}
    </section>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-bone-3 text-[10px] tracking-[0.18em] uppercase">{label}</span>
      {children}
    </label>
  );
}

function Input({
  value,
  onChange,
  type = "text",
  placeholder,
  mono,
}: {
  value: string;
  onChange: (v: string) => void;
  type?: string;
  placeholder?: string;
  mono?: boolean;
}) {
  const pulse = useAppStore((s) => s.pulse);
  return (
    <input
      type={type}
      value={value}
      placeholder={placeholder}
      onChange={(e) => {
        pulse();
        onChange(e.target.value);
      }}
      className={
        "w-full bg-ink-2/70 border border-ink-3 rounded-lg px-3.5 py-2.5 text-bone placeholder:text-bone-3/55 focus:outline-none focus:border-pulse/60 transition-colors " +
        (mono ? "font-mono text-sm" : "")
      }
    />
  );
}

function VoicePicker({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const activePreset = presetFromDescription(value);
  const isCustom =
    !activePreset && value.trim().length > 0;
  const [showCustom, setShowCustom] = useState(isCustom);
  const pulse = useAppStore((s) => s.pulse);
  return (
    <div className="flex flex-col gap-2">
      {VOICE_PRESETS.map((preset) => {
        const active = !isCustom && (activePreset?.id ?? "default") === preset.id;
        return (
          <button
            key={preset.id}
            type="button"
            onClick={() => {
              setShowCustom(false);
              onChange(preset.description);
            }}
            className={
              "text-left rounded-xl border px-4 py-3 transition-all " +
              (active
                ? "border-pulse/60 bg-pulse/[0.07]"
                : "border-ink-3 bg-ink-2/30 hover:border-ink-3/80 hover:bg-ink-2/50")
            }
          >
            <div className="flex items-center justify-between">
              <span className="text-bone font-medium">{preset.label}</span>
              {active && (
                <span className="h-1.5 w-1.5 rounded-full bg-pulse-2 shadow-[0_0_8px_rgba(110,196,232,0.7)]" />
              )}
            </div>
            <p className="text-bone-3 text-xs mt-0.5">{preset.blurb}</p>
          </button>
        );
      })}
      <button
        type="button"
        onClick={() => setShowCustom((v) => !v)}
        className={
          "text-left rounded-xl border px-4 py-3 transition-all " +
          (isCustom
            ? "border-pulse/60 bg-pulse/[0.07]"
            : "border-ink-3 bg-ink-2/30 hover:border-ink-3/80 hover:bg-ink-2/50")
        }
      >
        <div className="flex items-center justify-between">
          <span className="text-bone font-medium">Custom</span>
          {isCustom && (
            <span className="h-1.5 w-1.5 rounded-full bg-pulse-2 shadow-[0_0_8px_rgba(110,196,232,0.7)]" />
          )}
        </div>
        <p className="text-bone-3 text-xs mt-0.5">
          Write your own voice instructions.
        </p>
      </button>
      {showCustom && (
        <input
          autoFocus
          value={value}
          placeholder="e.g. blunt, no preamble, action verbs only"
          onChange={(e) => {
            pulse();
            onChange(e.target.value);
          }}
          className="w-full bg-ink-2/70 border border-ink-3 rounded-lg px-3.5 py-2.5 text-bone placeholder:text-bone-3/55 focus:outline-none focus:border-pulse/60 transition-colors mt-1"
        />
      )}
    </div>
  );
}

function TextArea({
  value,
  onChange,
  placeholder,
  rows = 4,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  rows?: number;
}) {
  const pulse = useAppStore((s) => s.pulse);
  return (
    <textarea
      value={value}
      placeholder={placeholder}
      rows={rows}
      onChange={(e) => {
        pulse();
        onChange(e.target.value);
      }}
      className="w-full bg-ink-2/70 border border-ink-3 rounded-lg px-3.5 py-2.5 text-bone placeholder:text-bone-3/55 focus:outline-none focus:border-pulse/60 transition-colors resize-none leading-relaxed"
    />
  );
}
