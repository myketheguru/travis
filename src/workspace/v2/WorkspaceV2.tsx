/**
 * WorkspaceV2 — canvas-first workspace with HUD overlays.
 *
 * The v2 redesign. Full-bleed canvas backdrop with ambient depth,
 * cards positioned by a focal-item layout engine, HUD elements at
 * fixed edge positions:
 *   TL: orb (activity states)
 *   TR: attention compass
 *   L:  active-thread rail
 *   R:  contextual action rail
 *   B:  command pill (bottom center)
 *
 * Video-game HUD reference: minimal chrome, focal point centered,
 * peripheral info that fades in when relevant. See INTERFACE.md v2
 * for the design principles.
 *
 * v2 Shell 1: this file scaffolds the surface + wires the settings
 * toggle. Subsequent shells (330-335) fill in each HUD element and
 * the canvas layout engine. Until then, this renders a placeholder
 * that clearly says 'v2 in progress' so users who opt in see what's
 * coming and can toggle back to classic.
 */
import { useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useAppStore } from "../../stores/app";
import { AttentionStrip } from "../AttentionStrip";
import { SuggestionRail } from "../SuggestionRail";
import { CanvasBackdrop } from "./CanvasBackdrop";
import { FocalStage } from "./FocalStage";
import { OrbitalStack } from "./OrbitalStack";
import { useFocalContent } from "./useFocalContent";
import AskTab from "../../manage/tabs/AskTab";

export function WorkspaceV2() {
  const setPendingComposerText = useAppStore((s) => s.setPendingComposerText);
  const activity = useAppStore((s) => s.activity);
  const { focal, orbits } = useFocalContent();

  // ⌘, opens Settings via the same route App.tsx handles today. Full
  // overlay lands in v2 Shell 6.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === ",") {
        e.preventDefault();
        // TODO(v2-shell-6): open Settings as overlay card
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="relative flex flex-col h-full min-h-0 overflow-hidden">
      {/* Canvas backdrop with ambient depth (v2 Shell 2) */}
      <CanvasBackdrop />

      {/* HUD: top-left orb (v2 Shell 4) */}
      <OrbHud />

      {/* HUD: top-right attention compass — for now, reuse the strip */}
      <div
        className="absolute top-3 right-3 z-20 pointer-events-auto"
        style={{ maxWidth: "60%" }}
      >
        <div className="rounded-full border border-white/10 bg-black/20 backdrop-blur">
          <AttentionStrip />
        </div>
      </div>

      {/* Primary content region: focal card + orbital stack + AskTab
          below. AskTab still owns the composer + chat scroll (v2
          Shell 4 will lift the composer into the HUD). */}
      <div className="relative z-10 flex-1 min-h-0 flex flex-col">
        <SuggestionRail
          onSuggestionClick={(s) => setPendingComposerText(s.prompt)}
        />
        <div className="flex-1 min-h-0 flex flex-col md:flex-row gap-4 px-6 py-4 overflow-auto">
          <div className="flex-1 min-w-0 flex items-start justify-center">
            <FocalStage message={focal} pending={activity === "thinking"} />
          </div>
          <OrbitalStack orbits={orbits} />
        </div>
        {/* Composer band — AskTab handles input; we hide its scroll
            using CSS since focal + orbits replace the visual reading
            experience for v2. */}
        <div
          className="shrink-0 border-t"
          style={{ borderColor: "rgba(255,255,255,0.06)" }}
        >
          <div className="max-h-[320px] overflow-hidden">
            <AskTab />
          </div>
        </div>
      </div>

      {/* v2 preview badge — visible until Shell 3-6 land */}
      <AnimatePresence>
        <motion.div
          key="v2-badge"
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.32, ease: [0.22, 1, 0.36, 1], delay: 0.2 }}
          className="absolute bottom-3 left-3 z-20"
        >
          <div
            className="text-[10px] font-mono uppercase tracking-[0.24em] px-2.5 py-1 rounded-full"
            style={{
              background: "rgba(124, 92, 255, 0.10)",
              color: "rgb(189, 158, 255)",
              border: "1px solid rgba(189, 158, 255, 0.35)",
            }}
            title="Canvas + HUD lands over the next few sprints. Toggle 'Interface' in Settings to switch back."
          >
            v2 preview
          </div>
        </motion.div>
      </AnimatePresence>
    </div>
  );
}

/* ─── Orb stub (v2 Shell 4) ─────────────────────────────────────── */

function OrbHud() {
  const activity = useAppStore((s) => s.activity);
  const pulseColor = pulseColorFor(activity);
  return (
    <div className="absolute top-3 left-3 z-20 pointer-events-none">
      <motion.div
        animate={{
          scale: activity === "thinking" || activity === "listening" ? [1, 1.08, 1] : 1,
        }}
        transition={{
          duration: 1.6,
          repeat: Infinity,
          ease: [0.22, 1, 0.36, 1],
        }}
        className="h-6 w-6 rounded-full"
        style={{
          background: `radial-gradient(circle at 35% 30%, ${pulseColor.core}, ${pulseColor.rim} 70%)`,
          boxShadow: `0 0 24px ${pulseColor.glow}`,
        }}
      />
    </div>
  );
}

function pulseColorFor(activity: string): {
  core: string;
  rim: string;
  glow: string;
} {
  switch (activity) {
    case "thinking":
      return {
        core: "rgba(189, 158, 255, 0.95)",
        rim: "rgba(124, 92, 255, 0.6)",
        glow: "rgba(124, 92, 255, 0.5)",
      };
    case "listening":
      return {
        core: "rgba(110, 196, 232, 0.95)",
        rim: "rgba(110, 196, 232, 0.5)",
        glow: "rgba(110, 196, 232, 0.5)",
      };
    case "speaking":
      return {
        core: "rgba(129, 199, 132, 0.95)",
        rim: "rgba(129, 199, 132, 0.5)",
        glow: "rgba(129, 199, 132, 0.5)",
      };
    case "typing":
      return {
        core: "rgba(255, 179, 92, 0.95)",
        rim: "rgba(255, 179, 92, 0.5)",
        glow: "rgba(255, 179, 92, 0.4)",
      };
    default:
      return {
        core: "rgba(236, 236, 241, 0.9)",
        rim: "rgba(236, 236, 241, 0.4)",
        glow: "rgba(236, 236, 241, 0.15)",
      };
  }
}
