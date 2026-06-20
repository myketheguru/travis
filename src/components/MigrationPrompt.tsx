/**
 * v2 Phase 2.1 — Migration prompt.
 *
 * Shown once for users who signed in for the first time and have
 * existing local data (a previous Travis install on this machine).
 *
 * Three explicit choices, no auto-default:
 *   - Upload my work   → pushes local data via /sync/push, then continues
 *   - Start fresh      → cloud begins empty, local is preserved offline
 *   - Skip for now     → asks again later
 */
import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { PresenceOrb } from "./PresenceOrb";
import {
  cloudMigrationSkip,
  cloudMigrationStartFresh,
  cloudMigrationStatus,
  cloudMigrationUpload,
  type LocalCounts,
  type MigrationDetails,
} from "../lib/cloud";

interface Props {
  onDone: () => void;
}

type Stage = "checking" | "ready" | "uploading" | "success" | "error";

export function MigrationPrompt({ onDone }: Props) {
  const [stage, setStage] = useState<Stage>("checking");
  const [counts, setCounts] = useState<LocalCounts | null>(null);
  const [result, setResult] = useState<MigrationDetails | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    cloudMigrationStatus()
      .then((s) => {
        setCounts(s.localCounts);
        setStage("ready");
      })
      .catch((e) => {
        setError(e instanceof Error ? e.message : String(e));
        setStage("error");
      });
  }, []);

  async function handleUpload() {
    setStage("uploading");
    setError(null);
    try {
      const r = await cloudMigrationUpload();
      setResult(r);
      setStage("success");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setStage("error");
    }
  }

  async function handleFresh() {
    try {
      await cloudMigrationStartFresh();
      onDone();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setStage("error");
    }
  }

  async function handleSkip() {
    try {
      await cloudMigrationSkip();
      onDone();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setStage("error");
    }
  }

  const totalEntities =
    (counts?.profile ?? 0) +
    (counts?.memories ?? 0) +
    (counts?.conversations ?? 0) +
    (counts?.settings ?? 0);

  return (
    <div className="min-h-screen flex items-center justify-center px-6 py-12">
      <motion.div
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, ease: [0.2, 0.8, 0.2, 1] }}
        className="w-full max-w-md text-center"
      >
        <div className="flex justify-center mb-7">
          <PresenceOrb size={120} />
        </div>

        {stage === "checking" && (
          <p className="text-bone-3 text-sm">Checking what's on this computer…</p>
        )}

        {stage === "ready" && counts && (
          <>
            <h1 className="text-bone text-3xl font-light tracking-tight mb-3">
              Bring your work with you?
            </h1>
            <p className="text-bone-3 text-sm leading-relaxed mb-7 max-w-sm mx-auto">
              We're moving Travis to the cloud so your work follows you across
              devices and keeps running when you're away. We found{" "}
              <span className="text-bone-2 font-medium">
                {totalEntities.toLocaleString()}
              </span>{" "}
              {totalEntities === 1 ? "item" : "items"} from your previous setup
              on this machine.
            </p>

            <div className="rounded-xl border border-ink-3 bg-ink-2/40 p-4 mb-7 text-left">
              <div className="text-bone-3 text-[10px] uppercase tracking-[0.18em] mb-2">
                On this computer
              </div>
              <CountRow label="Profile" n={counts.profile} />
              <CountRow label="Memories" n={counts.memories} />
              <CountRow
                label="Conversations"
                n={counts.conversations}
                sub={
                  counts.conversationMessages > 0
                    ? `${counts.conversationMessages.toLocaleString()} messages`
                    : undefined
                }
              />
              <CountRow label="Settings" n={counts.settings} />
            </div>

            <div className="flex flex-col gap-2.5">
              <button
                onClick={handleUpload}
                className="px-5 py-3 rounded-xl bg-bone text-ink font-medium text-sm transition-all hover:bg-white hover:-translate-y-px"
              >
                Upload my work to the cloud
              </button>
              <button
                onClick={handleFresh}
                className="px-5 py-3 rounded-xl border border-ink-3 bg-ink-2/30 text-bone-2 font-medium text-sm transition-all hover:bg-ink-2/50 hover:text-bone"
              >
                Start fresh in the cloud
              </button>
              <button
                onClick={handleSkip}
                className="text-bone-3 text-xs hover:text-bone-2 mt-1 transition-colors"
              >
                Skip for now — ask me later
              </button>
            </div>

            <p className="mt-6 text-[11px] text-bone-3 leading-relaxed max-w-xs mx-auto">
              You stay in control. Your local copy is preserved either way.
            </p>
          </>
        )}

        {stage === "uploading" && (
          <>
            <h1 className="text-bone text-2xl font-light tracking-tight mb-3">
              Bringing your work over…
            </h1>
            <p className="text-bone-3 text-sm leading-relaxed mb-2 max-w-sm mx-auto">
              This usually takes under a minute. Keep this window open.
            </p>
          </>
        )}

        {stage === "success" && result && (
          <>
            <h1 className="text-bone text-3xl font-light tracking-tight mb-3">
              You're all set.
            </h1>
            <p className="text-bone-3 text-sm leading-relaxed mb-6 max-w-sm mx-auto">
              {result.pushed.toLocaleString()} item
              {result.pushed === 1 ? "" : "s"} synced to the cloud. Your work
              now follows you across devices.
            </p>
            {result.skipped > 0 && (
              <p className="text-bone-3 text-[11px] mb-5 max-w-xs mx-auto">
                {result.skipped} item{result.skipped === 1 ? "" : "s"} couldn't
                be matched to a known type and stayed local — that's safe and
                expected for older data.
              </p>
            )}
            <button
              onClick={onDone}
              className="px-5 py-3 rounded-xl bg-bone text-ink font-medium text-sm transition-all hover:bg-white"
            >
              Continue to Travis
            </button>
          </>
        )}

        {stage === "error" && (
          <>
            <h1 className="text-bone text-2xl font-light tracking-tight mb-3">
              Something didn't go through.
            </h1>
            {error && (
              <div className="my-5 px-4 py-3 rounded-lg border border-warn/30 bg-warn/5 text-warn text-xs leading-relaxed">
                {error}
              </div>
            )}
            <div className="flex flex-col gap-2.5">
              <button
                onClick={() => {
                  setError(null);
                  setStage("ready");
                }}
                className="px-5 py-3 rounded-xl bg-bone text-ink font-medium text-sm hover:bg-white"
              >
                Try again
              </button>
              <button
                onClick={handleSkip}
                className="text-bone-3 text-xs hover:text-bone-2 mt-1"
              >
                Skip for now
              </button>
            </div>
          </>
        )}
      </motion.div>
    </div>
  );
}

function CountRow({
  label,
  n,
  sub,
}: {
  label: string;
  n: number;
  sub?: string;
}) {
  return (
    <div className="flex items-center justify-between py-1.5 text-sm">
      <span className="text-bone-2">{label}</span>
      <span className="text-bone font-mono">
        {n.toLocaleString()}
        {sub && <span className="text-bone-3 ml-2 text-xs">({sub})</span>}
      </span>
    </div>
  );
}
