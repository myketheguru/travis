/**
 * VoiceCanvas — v2 Shell 15.
 *
 * When Travis or the user is actively speaking, the canvas becomes:
 * spheroid centered, "Listening…" or "Speaking…" caption below.
 * Everything else on the canvas fades. The spheroid reacts to real
 * amplitude via the store (see SpeechScene / voice.ts wiring).
 */
import { motion } from "framer-motion";
import { useAppStore } from "../../../stores/app";
import { SpeechScene } from "../SpeechScene";

export function VoiceCanvas() {
  const activity = useAppStore((s) => s.activity);
  const caption = activity === "listening" ? "Listening…" : "Speaking…";

  return (
    <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-none">
      {/* SpeechScene already renders the full-bleed spheroid; we just
          add the caption. When VoiceCanvas mounts, activity is either
          listening or speaking — SpeechScene reacts automatically. */}
      <SpeechScene />
      <motion.div
        initial={{ opacity: 0, y: 6 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0 }}
        transition={{ duration: 0.32, ease: [0.22, 1, 0.36, 1] }}
        className="relative z-40 mt-[42vh] text-[13px] font-mono uppercase tracking-[0.26em]"
        style={{
          color: "rgba(236, 236, 241, 0.75)",
          textShadow: "0 0 24px rgba(0,0,0,0.6)",
        }}
      >
        {caption}
      </motion.div>
    </div>
  );
}
