import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import {
  getProactiveConfig,
  getShellEnabled,
  getUserProfile,
  hasApiKey,
  platformInfo,
  setApiKey,
  setProactiveEnabled,
  setProactiveSchedule,
  setShellEnabled,
  testProvider,
  updateProfile,
  type PingResult,
  type ProactiveConfig,
  type Provider,
  type UserProfile,
} from "../lib/ipc";
import { checkForUpdate, installUpdate, type UpdateInfo } from "../lib/updater";
import { listPacks, setPackEnabled, type PackInfo } from "../lib/packs";
import { exportData, revealExport, type ExportResult } from "../lib/dataExport";
// CompanySection (L2E-specific) was removed in v0.22.6 — packTableList /
// packTableUpsert no longer used in Settings. The L2E pack surfaces its
// own management UI through Manage's pack tabs when the pack is enabled.
import {
  archiveWorkspace,
  createWorkspace,
  isSensitive,
  listWorkspaces,
  unarchiveWorkspace,
  updateWorkspace,
  type Workspace,
  type WorkspaceCategory,
} from "../lib/workspaces";
import { VoiceDropdown } from "../components/VoiceDropdown";
import { useAppStore } from "../stores/app";
import {
  cloudConnectedAccounts,
  cloudDisconnectAccount,
  cloudExtendGoogleGrant,
  ensureInboxSummarySchedule,
  cloudPolicy,
  cloudSignOut,
  cloudStatus,
  cloudSyncNow,
  cloudSyncStatus,
  type ConnectedAccount,
  type CloudPolicy,
  type CloudUser,
  type SyncStatus,
} from "../lib/cloud";

const providers: { id: Provider; name: string; blurb: string; needsKey: boolean }[] = [
  { id: "travis_cloud", name: "Travis Cloud", blurb: "Managed by Travis — no API key needed",  needsKey: false },
  { id: "claude",       name: "Claude",       blurb: "Anthropic — your own API key",            needsKey: true },
  { id: "openai",       name: "OpenAI",       blurb: "GPT-class models, your own API key",      needsKey: true },
  { id: "ollama",       name: "Ollama",       blurb: "Run locally, private, no API key",         needsKey: false },
];

const defaultModels: Record<Provider, string> = {
  travis_cloud: "claude-sonnet-4-6",
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
  // v2 Phase 3 — cloudAvailable is gone (every signed-in user has Travis
  // Cloud; the build-time key check we used to do is irrelevant). Kept
  // setCloudAvailable as a no-op so older code paths setting it don't
  // need to be rewritten — TS picks it up as unused but it's referenced
  // in the platform_info load below.
  const setCloudAvailable = (_v: boolean) => {};
  const [useOwnLlm, setUseOwnLlm] = useState<boolean>(false);
  const [cloudUser, setCloudUser] = useState<CloudUser | null>(null);
  const [policy, setPolicy] = useState<CloudPolicy | null>(null);
  const [signingOut, setSigningOut] = useState(false);
  const [syncState, setSyncState] = useState<SyncStatus | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [syncFlash, setSyncFlash] = useState<string | null>(null);
  const setActivity = useAppStore((s) => s.setActivity);
  const setProfile = useAppStore((s) => s.setProfile);

  // Load the cloud account + policy + sync status in parallel with
  // the local profile. Sync status repolls every 10s so the indicator
  // stays fresh while Settings is open.
  useEffect(() => {
    cloudStatus()
      .then((s) => {
        if (s.signedIn && s.user) setCloudUser(s.user);
      })
      .catch(() => {});
    cloudPolicy()
      .then(setPolicy)
      .catch(() => {});
    cloudSyncStatus()
      .then(setSyncState)
      .catch(() => {});
    const t = setInterval(() => {
      cloudSyncStatus().then(setSyncState).catch(() => {});
    }, 10_000);
    return () => clearInterval(t);
  }, []);

  async function handleSyncNow() {
    if (syncing) return;
    setSyncing(true);
    setSyncFlash(null);
    try {
      const r = await cloudSyncNow();
      const parts: string[] = [];
      if (r.pushed > 0) parts.push(`${r.pushed} sent`);
      if (r.filesUploaded > 0)
        parts.push(`${r.filesUploaded} file${r.filesUploaded === 1 ? "" : "s"}`);
      if (r.pulledApplied > 0) parts.push(`${r.pulledApplied} received`);
      setSyncFlash(parts.length ? parts.join(" · ") : "Up to date");
      cloudSyncStatus().then(setSyncState).catch(() => {});
    } catch (e) {
      setSyncFlash(
        e instanceof Error ? e.message : "Sync failed — try again",
      );
    } finally {
      setSyncing(false);
      setTimeout(() => setSyncFlash(null), 4000);
    }
  }

  async function handleSignOut() {
    if (signingOut) return;
    setSigningOut(true);
    try {
      await cloudSignOut();
      // App.tsx's cloud gate re-checks on next render; reload window
      // forces it back to the SignIn screen cleanly.
      window.location.reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSigningOut(false);
    }
  }

  useEffect(() => {
    (async () => {
      const [p, info] = await Promise.all([getUserProfile(), platformInfo()]);
      setCloudAvailable(info.travisCloudAvailable);
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
      setUseOwnLlm(p.llmProvider !== "travis_cloud");
      if (p.llmProvider !== "ollama" && p.llmProvider !== "travis_cloud") {
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
        apiKey: draft.provider === "ollama" || draft.provider === "travis_cloud" ? undefined : draft.apiKey || undefined,
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
          draft.provider === "ollama" || draft.provider === "travis_cloud"
            ? undefined
            : draft.apiKey.trim() || undefined,
        ollamaUrl: draft.provider === "ollama" ? draft.ollamaUrl : undefined,

        model: draft.model || undefined,
      });

      if (draft.provider !== "ollama" && draft.provider !== "travis_cloud" && draft.apiKey.trim()) {
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
        draft.provider === "ollama" || draft.provider === "travis_cloud" ? false : await hasApiKey(draft.provider),
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
            Account, provider, model, and identity.
          </p>
        </div>

        {cloudUser && (
          <Section title="Connected">
            <ConnectedAccounts />
          </Section>
        )}

        {cloudUser && (
          <Section title="Account">
            <div className="rounded-xl border border-ink-3 bg-ink-2/40 p-4 flex flex-col gap-3">
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="text-bone text-sm font-medium truncate">
                    {cloudUser.name || cloudUser.email}
                  </div>
                  {cloudUser.name && (
                    <div className="text-bone-3 text-[11px] mt-0.5 truncate">
                      {cloudUser.email}
                    </div>
                  )}
                  <div className="flex items-center gap-2 mt-2">
                    <span
                      className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full border text-[10px] uppercase tracking-[0.14em]"
                      style={{
                        borderColor:
                          cloudUser.tier === "free"
                            ? "rgba(108,108,124,0.3)"
                            : "rgba(124,92,255,0.4)",
                        background:
                          cloudUser.tier === "free"
                            ? "rgba(108,108,124,0.08)"
                            : "rgba(124,92,255,0.07)",
                        color:
                          cloudUser.tier === "free"
                            ? "rgb(168, 168, 184)"
                            : "rgb(189, 158, 255)",
                      }}
                    >
                      <span
                        className="h-1.5 w-1.5 rounded-full"
                        style={{
                          background:
                            cloudUser.tier === "free"
                              ? "rgba(168, 168, 184, 0.7)"
                              : "rgba(189, 158, 255, 0.95)",
                        }}
                      />
                      {cloudUser.tier}
                    </span>
                  </div>
                </div>
                <button
                  onClick={handleSignOut}
                  disabled={signingOut}
                  className="shrink-0 text-bone-3 hover:text-bone-2 text-xs px-3 py-1.5 rounded-md border border-ink-3 hover:border-ink-3/80 transition-colors disabled:opacity-50"
                >
                  {signingOut ? "Signing out…" : "Sign out"}
                </button>
              </div>

              {syncState && (
                <div className="border-t border-ink-3 pt-3 flex flex-col gap-2 text-[11px] text-bone-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span
                        className={
                          "h-1.5 w-1.5 rounded-full " +
                          (syncState.failingOutbox > 0
                            ? "bg-warn"
                            : syncState.pendingOutbox > 0
                            ? "bg-pulse"
                            : "bg-pulse-2")
                        }
                        style={{
                          boxShadow:
                            syncState.failingOutbox > 0
                              ? "0 0 6px rgba(255,184,107,0.6)"
                              : syncState.pendingOutbox > 0
                              ? "0 0 6px rgba(124,92,255,0.6)"
                              : "0 0 6px rgba(110,196,232,0.6)",
                        }}
                      />
                      <span>
                        {syncState.failingOutbox > 0
                          ? `${syncState.failingOutbox} stuck`
                          : syncState.pendingOutbox > 0 || syncState.pendingFiles > 0
                          ? `${syncState.pendingOutbox + syncState.pendingFiles} queued`
                          : "Synced"}
                      </span>
                      {syncState.lastSyncAt && (
                        <span className="text-bone-3/70 ml-1 font-mono text-[10px]">
                          {fmtRelative(syncState.lastSyncAt)}
                        </span>
                      )}
                    </div>
                    <button
                      onClick={handleSyncNow}
                      disabled={syncing}
                      className="text-bone-3 hover:text-bone-2 text-[11px] px-2 py-0.5 rounded border border-ink-3 hover:border-ink-3/80 transition-colors disabled:opacity-50"
                    >
                      {syncing ? "syncing…" : "sync now"}
                    </button>
                  </div>
                  {syncFlash && (
                    <div className="text-bone-2 text-[10px] font-mono">
                      {syncFlash}
                    </div>
                  )}
                  {syncState.lastError && !syncFlash && (
                    <div className="text-warn text-[10px] leading-relaxed">
                      Last error: {syncState.lastError}
                    </div>
                  )}
                </div>
              )}

              {policy && (
                <div className="border-t border-ink-3 pt-3 flex flex-col gap-2 text-[11px] text-bone-3">
                  <div className="flex items-center justify-between">
                    <span>Today's usage</span>
                    <span className="font-mono text-bone-2">
                      ${(policy.usedToday.costCents / 100).toFixed(2)} of $
                      {(policy.dailyCostCapCents / 100).toFixed(2)}
                    </span>
                  </div>
                  <div className="h-1 rounded-full bg-ink-3 overflow-hidden">
                    <div
                      className="h-full bg-pulse-2 transition-all"
                      style={{
                        width: `${Math.min(
                          100,
                          (policy.usedToday.costCents /
                            Math.max(1, policy.dailyCostCapCents)) *
                            100,
                        )}%`,
                      }}
                    />
                  </div>
                  <div className="flex items-center justify-between">
                    <span>Calls</span>
                    <span className="font-mono text-bone-2">
                      {policy.usedToday.calls} of {policy.dailyCallCap}
                    </span>
                  </div>
                  {policy.allowedModels.length > 0 && (
                    <div className="flex items-center justify-between">
                      <span>Models on this plan</span>
                      <span className="font-mono text-bone-2">
                        {policy.allowedModels.join(", ")}
                      </span>
                    </div>
                  )}
                </div>
              )}
            </div>
          </Section>
        )}

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
          <Field label="Tone (optional)">
            <VoiceDropdown
              value={draft.communicationStyle}
              onChange={(v) => update({ communicationStyle: v })}
            />
          </Field>
        </Section>

        <Section title="Model">
          <div className="rounded-xl border border-pulse/40 bg-pulse/[0.05] p-3.5 flex flex-col gap-2">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <div className="text-bone font-medium text-sm">Travis Cloud</div>
                <p className="text-bone-3 text-[11px] mt-0.5 leading-relaxed">
                  Default. Hosted by Travis on your behalf — no key, no
                  setup. Tier policy and usage caps are enforced server-side
                  so your spend never overshoots your plan.
                </p>
              </div>
              <label className="flex items-center gap-2 cursor-pointer select-none shrink-0">
                <input
                  type="checkbox"
                  checked={!useOwnLlm}
                  onChange={(e) => {
                    const goCloud = e.target.checked;
                    setUseOwnLlm(!goCloud);
                    if (goCloud) {
                      update({ provider: "travis_cloud" as Provider });
                    } else if (draft.provider === "travis_cloud") {
                      update({ provider: "claude" as Provider });
                    }
                  }}
                  className="accent-pulse"
                />
                <span className="text-bone-2 text-xs">Use Travis Cloud</span>
              </label>
            </div>
          </div>

          {/* v2 Phase 3 — BYOK lives behind an Advanced disclosure.
              Default tier is Travis Cloud; we don't want a fresh user
              accidentally on a self-managed key path. */}
          <details className="rounded-xl border border-ink-3 bg-ink-2/20 group" open={useOwnLlm}>
            <summary className="cursor-pointer select-none px-4 py-2.5 text-bone-2 text-xs flex items-center justify-between hover:bg-ink-2/40 rounded-xl transition-colors">
              <span>Advanced — bring your own LLM</span>
              <span className="text-bone-3 group-open:rotate-90 transition-transform">›</span>
            </summary>
            <div className="px-4 pb-3 pt-1 flex flex-col gap-2">
              <p className="text-bone-3 text-[11px] leading-relaxed">
                Power-user mode: route LLM calls through your own provider
                key. Your sign-in still tracks usage so the cloud can
                rate-limit per account, but your provider charges you
                directly. Free tier users default to this path.
              </p>
              {providers
                .filter((p) => p.id !== "travis_cloud")
                .map((p) => {
                  const active = draft.provider === p.id;
                  return (
                    <button
                      key={p.id}
                      onClick={() => {
                        setUseOwnLlm(true);
                        update({ provider: p.id });
                      }}
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
          </details>

          {draft.provider === "travis_cloud" ? null : provider.needsKey ? (
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
                (draft.provider === "travis_cloud"
                  ? false
                  : provider.needsKey
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

        <WorkspacesSection />
        <PacksSection />
        <ProactiveSection />
        <ExportSection />
        <UpdatesSection />
        <CalendarSection />
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
  // v0.22.9 — Google rows removed; the cloud-managed ConnectedAccounts
  // section at the top of Settings now owns Gmail + Google Calendar
  // grant flow. This section keeps only providers that don't have a
  // cloud-side path yet (Microsoft 365). When/if cloud gets Microsoft
  // OAuth, this section can be removed entirely.
  return (
    <Section title="Microsoft (optional)">
      <p className="text-bone-3 text-[11px] leading-relaxed -mt-2">
        Connect Outlook so Travis can see your calendar and send mail on
        your behalf — only when you confirm a preview card. Calendar
        access is read-only. (For Google, use Connected above.)
      </p>
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

  // Wrapped in an Advanced disclosure — niche power-user feature; most
  // users never need it. Opens automatically if the toggle is on.
  return (
    <Section title="Advanced">
      <details
        className="rounded-xl border border-ink-3 bg-ink-2/20 group"
        open={enabled === true}
      >
        <summary className="cursor-pointer select-none px-4 py-2.5 text-bone-2 text-xs flex items-center justify-between hover:bg-ink-2/40 rounded-xl transition-colors">
          <span>Computer access — let Travis run shell commands</span>
          <span className="text-bone-3 group-open:rotate-90 transition-transform">›</span>
        </summary>
        <div className="px-4 pb-4 pt-1 flex flex-col gap-3">
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
        </div>
      </details>
    </Section>
  );
}

// Mon=1..Sun=7, ordered to match the way most people think about a week.
const DAY_CHIPS: { num: number; label: string }[] = [
  { num: 1, label: "Mon" },
  { num: 2, label: "Tue" },
  { num: 3, label: "Wed" },
  { num: 4, label: "Thu" },
  { num: 5, label: "Fri" },
  { num: 6, label: "Sat" },
  { num: 7, label: "Sun" },
];

const HOUR_OPTIONS = Array.from({ length: 25 }, (_, i) => i);

const fmtHour = (h: number) => {
  if (h === 0 || h === 24) return "midnight";
  if (h === 12) return "noon";
  return h < 12 ? `${h} am` : `${h - 12} pm`;
};

function ProactiveSection() {
  const [cfg, setCfg] = useState<ProactiveConfig | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [savedHint, setSavedHint] = useState<string | null>(null);

  useEffect(() => {
    getProactiveConfig()
      .then(setCfg)
      .catch(() =>
        setCfg({
          enabled: false,
          lastAt: null,
          activeDays: [1, 2, 3, 4, 5, 6, 7],
          startHour: 8,
          endHour: 22,
        }),
      );
  }, []);

  const toggle = async (v: boolean) => {
    setBusy(true);
    setErr(null);
    try {
      await setProactiveEnabled(v);
      setCfg((c) => (c ? { ...c, enabled: v } : null));
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const toggleDay = (n: number) => {
    if (!cfg) return;
    const next = cfg.activeDays.includes(n)
      ? cfg.activeDays.filter((d) => d !== n)
      : [...cfg.activeDays, n].sort((a, b) => a - b);
    setCfg({ ...cfg, activeDays: next });
  };

  const setHourField = (which: "startHour" | "endHour", val: number) => {
    if (!cfg) return;
    setCfg({ ...cfg, [which]: val });
  };

  const saveSchedule = async () => {
    if (!cfg) return;
    setBusy(true);
    setErr(null);
    setSavedHint(null);
    try {
      await setProactiveSchedule({
        activeDays: cfg.activeDays,
        startHour: cfg.startHour,
        endHour: cfg.endHour,
      });
      setSavedHint("Saved.");
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const presetWeekdays = () => {
    if (!cfg) return;
    setCfg({ ...cfg, activeDays: [1, 2, 3, 4, 5] });
  };
  const presetEveryDay = () => {
    if (!cfg) return;
    setCfg({ ...cfg, activeDays: [1, 2, 3, 4, 5, 6, 7] });
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
        Travis quietly checks in during the hours you choose — but only when
        there's something specific worth surfacing, like an overdue task, a
        thread waiting on you, or a capability gap. Stays silent otherwise.
        On by default — turn off if it's too chatty.
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

      {cfg && (
        <div className="mt-2 flex flex-col gap-3 rounded-xl border border-ink-3 bg-ink-2/30 p-3.5">
          <div>
            <div className="text-bone-3 text-[10px] tracking-[0.18em] uppercase mb-2">
              Active days
            </div>
            <div className="flex flex-wrap gap-1.5">
              {DAY_CHIPS.map((d) => {
                const on = cfg.activeDays.includes(d.num);
                return (
                  <button
                    key={d.num}
                    type="button"
                    onClick={() => toggleDay(d.num)}
                    disabled={busy}
                    className={
                      "px-3 py-1.5 rounded-full text-[11px] tracking-wider transition-colors disabled:opacity-50 " +
                      (on
                        ? "bg-pulse/20 text-bone border border-pulse/40"
                        : "border border-ink-3 text-bone-3 hover:text-bone-2 hover:border-ink-3/80")
                    }
                  >
                    {d.label}
                  </button>
                );
              })}
            </div>
            <div className="flex items-center gap-3 mt-2 text-[10px]">
              <button
                type="button"
                onClick={presetWeekdays}
                disabled={busy}
                className="text-bone-3 hover:text-bone-2 underline-offset-4 hover:underline"
              >
                Weekdays only
              </button>
              <span className="text-bone-3 opacity-50">·</span>
              <button
                type="button"
                onClick={presetEveryDay}
                disabled={busy}
                className="text-bone-3 hover:text-bone-2 underline-offset-4 hover:underline"
              >
                Every day
              </button>
            </div>
          </div>

          <div className="flex items-end gap-3">
            <div className="flex-1">
              <div className="text-bone-3 text-[10px] tracking-[0.18em] uppercase mb-1.5">
                From
              </div>
              <select
                value={cfg.startHour}
                onChange={(e) => setHourField("startHour", Number(e.target.value))}
                disabled={busy}
                className="w-full bg-ink-2/70 border border-ink-3 rounded-lg px-3 py-2 text-bone text-sm focus:outline-none focus:border-pulse/60 transition-colors"
              >
                {HOUR_OPTIONS.map((h) => (
                  <option key={h} value={h}>
                    {fmtHour(h)}
                  </option>
                ))}
              </select>
            </div>
            <div className="flex-1">
              <div className="text-bone-3 text-[10px] tracking-[0.18em] uppercase mb-1.5">
                Until
              </div>
              <select
                value={cfg.endHour}
                onChange={(e) => setHourField("endHour", Number(e.target.value))}
                disabled={busy}
                className="w-full bg-ink-2/70 border border-ink-3 rounded-lg px-3 py-2 text-bone text-sm focus:outline-none focus:border-pulse/60 transition-colors"
              >
                {HOUR_OPTIONS.map((h) => (
                  <option key={h} value={h}>
                    {fmtHour(h)}
                  </option>
                ))}
              </select>
            </div>
          </div>
          <p className="text-bone-3 text-[10px] leading-relaxed">
            If "until" is earlier than "from" the schedule wraps overnight —
            e.g. 10 pm → 6 am works.
          </p>

          <div className="flex items-center gap-3">
            <button
              onClick={saveSchedule}
              disabled={busy || cfg.activeDays.length === 0}
              className="px-4 py-1.5 rounded-full bg-bone/95 text-ink text-[11px] font-medium hover:bg-bone disabled:opacity-30 transition-colors"
            >
              {busy ? "Saving…" : "Save schedule"}
            </button>
            {savedHint && (
              <span className="text-pulse-2 text-[11px]">{savedHint}</span>
            )}
          </div>
        </div>
      )}

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

function WorkspacesSection() {
  const [workspaces, setWorkspaces] = useState<Workspace[] | null>(null);
  const [editing, setEditing] = useState<Workspace | null>(null);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const list = await listWorkspaces();
      setWorkspaces(list);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const onArchive = async (id: number) => {
    setError(null);
    try {
      await archiveWorkspace(id);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const onUnarchive = async (id: number) => {
    setError(null);
    try {
      await unarchiveWorkspace(id);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  if (creating || editing) {
    return (
      <Section title="Workspaces">
        <WorkspaceForm
          existing={editing ?? undefined}
          onCancel={() => {
            setCreating(false);
            setEditing(null);
            setError(null);
          }}
          onSaved={() => {
            setCreating(false);
            setEditing(null);
            refresh();
          }}
        />
        {error && <p className="text-warn text-[11px]">{error}</p>}
      </Section>
    );
  }

  const active = (workspaces ?? []).filter((w) => !w.archivedAt);
  const archived = (workspaces ?? []).filter((w) => w.archivedAt);

  return (
    <Section title="Workspaces">
      <p className="text-bone-3 text-[11px] leading-relaxed -mt-2">
        Workspaces are scoped namespaces for your data — Work, Personal, side
        projects. Sensitive categories (Health, Therapy, Legal, Finance) are
        isolated by default; data never crosses into other workspaces unless
        you explicitly toggle it.
      </p>

      {!workspaces && !error && (
        <p className="text-bone-3 text-xs">Loading…</p>
      )}

      {workspaces && (
        <div className="flex flex-col gap-2">
          {active.map((w) => (
            <WorkspaceRow
              key={w.id}
              w={w}
              onEdit={() => setEditing(w)}
              onArchive={() => onArchive(w.id)}
              onUnarchive={() => onUnarchive(w.id)}
            />
          ))}

          {archived.length > 0 && (
            <details className="mt-2">
              <summary className="text-bone-3 text-[11px] cursor-pointer hover:text-bone-2">
                {archived.length} archived
              </summary>
              <div className="mt-2 flex flex-col gap-2">
                {archived.map((w) => (
                  <WorkspaceRow
                    key={w.id}
                    w={w}
                    onEdit={() => setEditing(w)}
                    onArchive={() => onArchive(w.id)}
                    onUnarchive={() => onUnarchive(w.id)}
                  />
                ))}
              </div>
            </details>
          )}

          <button
            onClick={() => setCreating(true)}
            className="mt-2 px-3 py-1.5 rounded-full bg-pulse/15 border border-pulse/40 text-bone-2 text-xs hover:bg-pulse/25 transition-colors self-start"
          >
            + New workspace
          </button>
        </div>
      )}

      {error && <p className="text-warn text-[11px]">{error}</p>}
    </Section>
  );
}

function WorkspaceRow({
  w,
  onEdit,
  onArchive,
  onUnarchive,
}: {
  w: Workspace;
  onEdit: () => void;
  onArchive: () => void;
  onUnarchive: () => void;
}) {
  const sensitive = isSensitive(w.category);
  const archived = !!w.archivedAt;

  return (
    <div
      className={
        "flex items-center gap-3 rounded-xl border px-4 py-3 " +
        (archived
          ? "border-ink-3 bg-ink-2/20 opacity-60"
          : sensitive
          ? "border-warn/30 bg-warn/[0.04]"
          : "border-ink-3 bg-ink-2/30")
      }
    >
      <div className="flex-1 min-w-0">
        <div className="flex items-baseline gap-2">
          {sensitive && <span className="text-warn text-[10px]">🔒</span>}
          <span className="text-bone text-sm font-medium">{w.name}</span>
          <span className="text-bone-3 text-[10px] uppercase tracking-wider">
            {w.category}
          </span>
          {!sensitive && w.crossVisible !== 0 && (
            <span className="text-pulse-2 text-[10px] tracking-wider opacity-70">
              cross-visible
            </span>
          )}
        </div>
        <p className="text-bone-3 text-[10px] mt-0.5 font-mono opacity-50">
          {w.slug}
        </p>
      </div>
      <div className="flex items-center gap-1.5">
        <button
          onClick={onEdit}
          disabled={archived}
          className="px-2.5 py-1 rounded-full border border-ink-3 hover:border-pulse/40 text-bone-3 hover:text-bone-2 text-[11px] transition-colors disabled:opacity-30"
        >
          Edit
        </button>
        {archived ? (
          <button
            onClick={onUnarchive}
            className="px-2.5 py-1 rounded-full border border-ink-3 hover:border-pulse-2/40 text-bone-3 hover:text-pulse-2 text-[11px] transition-colors"
          >
            Restore
          </button>
        ) : w.id !== 1 ? (
          <button
            onClick={onArchive}
            className="px-2.5 py-1 rounded-full border border-ink-3 hover:border-warn/40 text-bone-3 hover:text-warn text-[11px] transition-colors"
          >
            Archive
          </button>
        ) : (
          <span className="text-bone-3 text-[10px] px-2.5 opacity-50">default</span>
        )}
      </div>
    </div>
  );
}

const CATEGORIES: { id: WorkspaceCategory; label: string; sensitive: boolean }[] = [
  { id: "work",     label: "Work",     sensitive: false },
  { id: "personal", label: "Personal", sensitive: false },
  { id: "other",    label: "Other",    sensitive: false },
  { id: "health",   label: "Health",   sensitive: true },
  { id: "therapy",  label: "Therapy",  sensitive: true },
  { id: "legal",    label: "Legal",    sensitive: true },
  { id: "finance",  label: "Finance",  sensitive: true },
];

function WorkspaceForm({
  existing,
  onCancel,
  onSaved,
}: {
  existing?: Workspace;
  onCancel: () => void;
  onSaved: () => void;
}) {
  const [name, setName] = useState(existing?.name ?? "");
  const [category, setCategory] = useState<WorkspaceCategory>(
    existing?.category ?? "personal",
  );
  const [crossVisible, setCrossVisible] = useState(
    existing ? existing.crossVisible !== 0 : !isSensitive("personal"),
  );
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // When the user picks a different category, reset crossVisible to
  // the new category's default (only on create — don't override an
  // existing user choice).
  const setCategoryAndDefault = (next: WorkspaceCategory) => {
    setCategory(next);
    if (!existing) setCrossVisible(!isSensitive(next));
  };

  const save = async () => {
    setError(null);
    if (!name.trim()) {
      setError("Name is required.");
      return;
    }
    setSaving(true);
    try {
      if (existing) {
        await updateWorkspace(existing.id, {
          name: name.trim(),
          category,
          crossVisible,
        });
      } else {
        await createWorkspace({
          name: name.trim(),
          category,
          crossVisible,
        });
      }
      onSaved();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const sensitive = isSensitive(category);

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-baseline justify-between">
        <span className="text-bone text-sm font-medium">
          {existing ? `Edit ${existing.name}` : "New workspace"}
        </span>
        <button
          onClick={onCancel}
          className="text-bone-3 hover:text-bone-2 text-xs"
        >
          Cancel
        </button>
      </div>

      <Field label="Name">
        <Input value={name} onChange={setName} />
      </Field>

      <Field label="Category">
        <div className="grid grid-cols-2 gap-2">
          {CATEGORIES.map((c) => {
            const selected = c.id === category;
            return (
              <button
                key={c.id}
                onClick={() => setCategoryAndDefault(c.id)}
                className={
                  "rounded-xl border px-3 py-2 text-left text-sm transition-all " +
                  (selected
                    ? c.sensitive
                      ? "border-warn/60 bg-warn/[0.07]"
                      : "border-pulse/60 bg-pulse/[0.07]"
                    : "border-ink-3 bg-ink-2/30 hover:border-ink-3/80 hover:bg-ink-2/50")
                }
              >
                <div className="flex items-center gap-2">
                  {c.sensitive && <span className="text-warn">🔒</span>}
                  <span className="text-bone-2">{c.label}</span>
                </div>
                {c.sensitive && (
                  <p className="text-bone-3 text-[10px] mt-0.5 leading-snug">
                    Isolated by default
                  </p>
                )}
              </button>
            );
          })}
        </div>
      </Field>

      <Field label="Cross-workspace visibility">
        <label className="flex items-center gap-2 cursor-pointer select-none">
          <input
            type="checkbox"
            checked={crossVisible}
            onChange={(e) => setCrossVisible(e.target.checked)}
            className="accent-pulse"
          />
          <span className="text-bone-2 text-sm">
            Show this workspace's data in other workspaces' views
          </span>
        </label>
        {sensitive && crossVisible && (
          <p className="text-warn text-[11px] mt-2 leading-relaxed">
            Sensitive workspaces are isolated by default. Toggling this on
            means data from this workspace will appear in cross-workspace
            queries from your other (non-sensitive) workspaces. Continue
            only if you're sure.
          </p>
        )}
      </Field>

      {error && <p className="text-warn text-[11px]">{error}</p>}

      <div className="flex items-center gap-3 pt-1">
        <button
          onClick={save}
          disabled={saving}
          className="px-4 py-2 rounded-full bg-bone/95 text-ink text-sm font-medium hover:bg-bone disabled:opacity-50 transition-colors"
        >
          {saving ? "Saving…" : existing ? "Save" : "Create"}
        </button>
      </div>
    </div>
  );
}

function PacksSection() {
  const [packs, setPacks] = useState<PackInfo[] | null>(null);
  const [pending, setPending] = useState<Set<string>>(new Set());
  const [restartHint, setRestartHint] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listPacks().then(setPacks).catch((e) => setError(String(e)));
  }, []);

  const toggle = async (slug: string, enabled: boolean) => {
    setPending((s) => new Set(s).add(slug));
    setError(null);
    try {
      await setPackEnabled(slug, enabled);
      setPacks((prev) =>
        prev ? prev.map((p) => (p.slug === slug ? { ...p, enabled } : p)) : prev,
      );
      setRestartHint(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setPending((s) => {
        const next = new Set(s);
        next.delete(slug);
        return next;
      });
    }
  };

  // Entitlement gating — Settings → Packs is for managing what you
  // already have, NOT for discovering what you could add. A pack
  // becomes "managed" once it's enabled (by org sync, by grandfather
  // migration, or by a previous user toggle). For users with no
  // enabled packs, the section is hidden entirely — adding access
  // happens via Org admin, not self-service from a Free tier.
  const managed = packs?.filter((p) => p.enabled) ?? null;
  if (managed !== null && managed.length === 0) return null;

  return (
    <Section title="Packs">
      <p className="text-bone-3 text-[11px] leading-relaxed -mt-2">
        Add-ons your account has access to. Turn them off here to hide
        their tabs, prompts, and tools — your data stays put either way.
      </p>
      {!packs && !error && (
        <p className="text-bone-3 text-xs">Loading…</p>
      )}
      {managed && managed.length > 0 && (
        <div className="flex flex-col gap-2">
          {managed.map((p) => {
            const isPending = pending.has(p.slug);
            return (
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
                  disabled={isPending}
                  onChange={(e) => toggle(p.slug, e.target.checked)}
                  className="accent-pulse mt-0.5"
                />
                <div className="flex-1 min-w-0">
                  <div className="flex items-baseline gap-2">
                    <span className="text-bone text-sm">{p.name}</span>
                    <span className="text-bone-3 text-[10px] font-mono opacity-70">
                      v{p.version}
                    </span>
                  </div>
                  {p.description && (
                    <p className="text-bone-3 text-[11px] mt-1 leading-relaxed">
                      {p.description}
                    </p>
                  )}
                </div>
              </label>
            );
          })}
        </div>
      )}
      {restartHint && (
        <p className="text-pulse-2 text-[11px]">
          Changes take effect on next launch.
        </p>
      )}
      {error && <p className="text-warn text-[11px]">{error}</p>}
    </Section>
  );
}

// DiagnosticsSection (Manage tracking-tab reveal) was removed in v0.22.9.
// Entity/Summary/Ask state is internal — not something users should be
// browsing or toggling from Settings. The diagnostics toggle remains in
// the store for dev-only flips (e.g., via console), but it no longer
// has a user-facing surface.

// CompanySection (L2E-pack-specific company profile editor) was removed
// in v0.22.6. The original direction was that pack-specific surfaces
// don't live in core Settings at all — the L2E pack uses its own
// Manage tabs (when the pack is enabled) to expose company-profile
// management. Core Settings stays domain-neutral.

function ExportSection() {
  const [busy, setBusy] = useState(false);
  const [includeSensitive, setIncludeSensitive] = useState(false);
  const [result, setResult] = useState<ExportResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const onExport = async () => {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const r = await exportData(includeSensitive);
      setResult(r);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const onReveal = async () => {
    if (!result) return;
    try {
      await revealExport(result.path);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const formatSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  const totalRows = result
    ? Object.values(result.tableRowCounts).reduce((acc, n) => acc + n, 0)
    : 0;
  const sortedTables = result
    ? Object.entries(result.tableRowCounts)
        .filter(([, n]) => n > 0)
        .sort((a, b) => b[1] - a[1])
    : [];

  return (
    <Section title="Export">
      <div className="flex flex-col gap-3 max-w-xl">
        <p className="text-bone-3 text-[11px] leading-relaxed">
          A snapshot of everything Travis is holding for you on this
          device — journal entries, tasks, reminders, conversations,
          pack-typed records — bundled into one JSON file you can read,
          archive, or share with someone you trust. API keys and OAuth
          tokens are stripped automatically.
        </p>

        <label className="flex items-start gap-2 cursor-pointer select-none">
          <input
            type="checkbox"
            checked={includeSensitive}
            onChange={(e) => setIncludeSensitive(e.target.checked)}
            className="accent-pulse mt-0.5"
          />
          <span className="text-bone-2 text-[12px] leading-snug">
            Include sensitive workspaces (Health, Therapy, Legal, Finance).
            <span className="text-bone-3 text-[10px] block mt-0.5">
              Off by default — sensitive workspaces are isolated unless you
              explicitly opt in. Turn on if you want a complete picture.
            </span>
          </span>
        </label>

        <div className="flex items-center gap-3">
          <button
            onClick={onExport}
            disabled={busy}
            className="px-4 py-1.5 rounded-full bg-bone/95 text-ink text-xs font-medium hover:bg-bone disabled:opacity-50 transition-colors"
          >
            {busy ? "Building export…" : "Export everything"}
          </button>
          {result && (
            <button
              onClick={onReveal}
              className="text-pulse-2 hover:text-bone text-[11px] underline-offset-4 hover:underline transition-colors"
            >
              Reveal in folder
            </button>
          )}
        </div>

        {error && <p className="text-warn text-[11px]">{error}</p>}

        {result && (
          <div className="rounded-lg border border-ink-3 bg-ink-2/30 p-3 text-[11px] flex flex-col gap-2">
            <div className="flex items-baseline justify-between gap-3">
              <code className="text-bone-2 truncate font-mono text-[10px]">
                {result.path}
              </code>
              <span className="text-bone-3 shrink-0">
                {formatSize(result.sizeBytes)} · {totalRows.toLocaleString()} rows
              </span>
            </div>
            {sortedTables.length > 0 && (
              <details className="text-bone-3">
                <summary className="cursor-pointer hover:text-bone-2">
                  What's inside
                </summary>
                <div className="mt-2 grid grid-cols-2 gap-x-4 gap-y-0.5">
                  {sortedTables.map(([table, count]) => (
                    <div key={table} className="flex justify-between">
                      <span>{table}</span>
                      <span className="font-mono">{count}</span>
                    </div>
                  ))}
                </div>
              </details>
            )}
            {result.redactions.length > 0 && (
              <div className="text-bone-3 leading-relaxed">
                <span className="text-bone-2">Notes:</span>{" "}
                {result.redactions.join(" · ")}
              </div>
            )}
          </div>
        )}
      </div>
    </Section>
  );
}

function fmtRelative(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "";
  const diffSec = Math.max(0, Math.round((Date.now() - t) / 1000));
  if (diffSec < 5) return "just now";
  if (diffSec < 60) return `${diffSec}s ago`;
  const min = Math.round(diffSec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const d = Math.round(hr / 24);
  return `${d}d ago`;
}

/** v0.21.5 Tier 2 — Settings card surfacing the user's connected
 *  Google services (inbox + calendar). One toggle per surface; when
 *  not enrolled, Connect kicks off the loopback OAuth-extend flow. */
function ConnectedAccounts() {
  const [accounts, setAccounts] = useState<ConnectedAccount[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function load() {
    try {
      const a = await cloudConnectedAccounts();
      setAccounts(a);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }
  useEffect(() => {
    void load();
  }, []);

  const isConnected = (provider: string) =>
    accounts?.some((a) => a.provider === provider && a.is_active === 1) ?? false;

  async function connect(provider: "gmail" | "gcal") {
    setBusy(provider);
    setError(null);
    try {
      await cloudExtendGoogleGrant([provider]);
      // v0.21.6 — auto-create the recurring inbox summary schedule the
      // first time the user enrolls Gmail. Without this, Connect just
      // grants access — nothing actually fires until the user manually
      // POSTs /workflows/schedules. Cloud roundtrip is cheap; the
      // helper is idempotent (looks up existing first).
      if (provider === "gmail") {
        try {
          await ensureInboxSummarySchedule();
        } catch (e) {
          tracing(e);
        }
      }
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  function tracing(e: unknown) {
    // Console-only; the Connect itself succeeded so we don't want to
    // bubble a follow-up failure into the UI as an error state.
    console.warn("auto-create inbox schedule failed (non-fatal):", e);
  }

  async function disconnect(provider: string) {
    setBusy(provider);
    setError(null);
    try {
      await cloudDisconnectAccount(provider);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="rounded-xl border border-ink-3 bg-ink-2/40 p-4 flex flex-col gap-3">
      <p className="text-bone-3 text-[11px] leading-relaxed">
        Let Travis read your inbox + calendar in the cloud so it can summarize
        what came in while you were away. Send actions are still local — Travis
        never sends mail without your approval.
      </p>
      {[
        { id: "gmail" as const, label: "Inbox (Gmail)", desc: "Read recent mail; classify urgency; draft replies for approval." },
        { id: "gcal" as const, label: "Calendar (Google)", desc: "Read upcoming events; pull context into meeting prep." },
      ].map((row) => {
        const on = isConnected(row.id);
        return (
          <div
            key={row.id}
            className="flex items-start justify-between gap-3 border-t border-ink-3 pt-3 first:border-t-0 first:pt-0"
          >
            <div className="min-w-0">
              <div className="text-bone-2 text-xs font-medium">{row.label}</div>
              <div className="text-bone-3 text-[11px] mt-1 leading-relaxed">
                {row.desc}
              </div>
            </div>
            <button
              onClick={() => (on ? disconnect(row.id) : connect(row.id))}
              disabled={busy === row.id}
              className={
                "shrink-0 text-xs px-3 py-1.5 rounded-md transition-colors disabled:opacity-50 " +
                (on
                  ? "border border-ink-3 text-bone-2 hover:border-ink-3/80"
                  : "bg-bone text-ink hover:bg-white font-medium")
              }
            >
              {busy === row.id
                ? on
                  ? "Disconnecting…"
                  : "Connecting…"
                : on
                ? "Disconnect"
                : "Connect"}
            </button>
          </div>
        );
      })}
      {error && (
        <div className="text-warn text-[11px] leading-relaxed">{error}</div>
      )}
    </div>
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
