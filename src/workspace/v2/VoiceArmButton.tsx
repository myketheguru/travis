/**
 * VoiceArmButton — v0.28.2.
 *
 * Replaces the old VoiceInputButton in the Composer. Tapping it fires
 * `travis:arm-voice`, which:
 *   - plays the wake cue
 *   - sets activity=listening (spheroid appears)
 *   - arms the native cpal capture for the next utterance
 * Tapping again while armed disarms.
 *
 * No mic ownership here — the mic is already live via useNativeVoice.
 * This button is just a mode switch.
 */
import { motion } from "framer-motion";
import { useAppStore } from "../../stores/app";

interface Props {
  disabled?: boolean;
}

export function VoiceArmButton({ disabled }: Props) {
  const activity = useAppStore((s) => s.activity);
  const isArmed = activity === "listening";

  function onClick(e: React.MouseEvent<HTMLButtonElement>) {
    e.preventDefault();
    if (isArmed) {
      window.dispatchEvent(new CustomEvent("travis:disarm-voice"));
    } else {
      window.dispatchEvent(new CustomEvent("travis:arm-voice"));
    }
  }

  return (
    <motion.button
      onClick={onClick}
      disabled={disabled}
      whileHover={disabled ? {} : { scale: 1.06 }}
      whileTap={disabled ? {} : { scale: 0.94 }}
      className="shrink-0 h-9 w-9 flex items-center justify-center rounded-full transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
      style={{
        background: isArmed
          ? "rgba(255, 107, 107, 0.20)"
          : "rgba(255, 255, 255, 0.04)",
        border: `1px solid ${
          isArmed ? "rgba(255, 107, 107, 0.55)" : "rgba(255, 255, 255, 0.14)"
        }`,
        color: isArmed ? "rgb(255, 107, 107)" : "rgba(236, 236, 241, 0.7)",
      }}
      title={isArmed ? "Listening — tap to stop" : "Tap to speak"}
      aria-label={isArmed ? "Stop listening" : "Speak"}
    >
      {isArmed ? (
        <motion.span
          animate={{ opacity: [1, 0.4, 1] }}
          transition={{ duration: 1.1, repeat: Infinity, ease: [0.42, 0, 0.58, 1] }}
          className="block h-2 w-2 rounded-full"
          style={{
            background: "rgb(255, 107, 107)",
            boxShadow: "0 0 10px rgba(255, 107, 107, 0.7)",
          }}
        />
      ) : (
        <svg
          viewBox="0 0 20 20"
          width="14"
          height="14"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
        >
          <path d="M10 3a3 3 0 0 0-3 3v4a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3z" />
          <path d="M5 10v1a5 5 0 0 0 10 0v-1M10 16v2" />
        </svg>
      )}
    </motion.button>
  );
}
