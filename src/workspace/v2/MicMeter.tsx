/**
 * MicMeter — v0.27.5.
 *
 * Live audio level meter shown inside the Composer while the mic is
 * armed. Reads the store's speechAmplitude (set by VoiceInputButton on
 * every audio-process tick) and renders 8 vertical bars whose heights
 * ripple with the current level. Silent means no ambient noise; the
 * meter answers 'is my mic actually picking anything up' at a glance.
 */
import { motion } from "framer-motion";
import { useAppStore } from "../../stores/app";

const BAR_COUNT = 8;

export function MicMeter() {
  const amplitude = useAppStore((s) => s.speechAmplitude);

  return (
    <div
      className="flex items-end gap-[3px] h-6"
      role="meter"
      aria-label="Microphone level"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(amplitude * 100)}
    >
      {Array.from({ length: BAR_COUNT }).map((_, i) => {
        const localThreshold = (i + 1) / BAR_COUNT;
        const on = amplitude >= localThreshold * 0.6;
        return (
          <motion.span
            key={i}
            animate={{
              height: on
                ? `${20 + amplitude * 80 * (0.6 + 0.4 * (i / BAR_COUNT))}%`
                : "18%",
              opacity: on ? 0.95 : 0.4,
            }}
            transition={{ duration: 0.12, ease: [0.22, 1, 0.36, 1] }}
            className="w-[3px] rounded-full"
            style={{
              background: on
                ? "rgb(189, 158, 255)"
                : "rgba(236, 236, 241, 0.25)",
              boxShadow: on
                ? "0 0 8px rgba(189, 158, 255, 0.5)"
                : "none",
            }}
          />
        );
      })}
    </div>
  );
}
