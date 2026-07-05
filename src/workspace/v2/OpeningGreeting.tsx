/**
 * OpeningGreeting — v2 Shell 8.
 *
 * The old splash content, now on the canvas. Renders 'Hi, {first}. I'm
 * here.' with a subtle glow underneath. Reads isFirstMoment from the
 * app store; when it goes false (user typed / clicked / activity), the
 * greeting fades out via the parent's AnimatePresence.
 *
 * Shown only when isFirstMoment is true — cold boot or 24h+ idle.
 */
import { motion } from "framer-motion";
import { useAppStore } from "../../stores/app";

export function OpeningGreeting() {
  const profile = useAppStore((s) => s.profile);
  const first = firstNameOf(profile?.name);

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -12, scale: 0.98 }}
      transition={{ duration: 0.6, ease: [0.22, 1, 0.36, 1] }}
      className="text-center pointer-events-none select-none"
    >
      <motion.div
        animate={{
          textShadow: [
            "0 0 24px rgba(124, 92, 255, 0.15)",
            "0 0 40px rgba(124, 92, 255, 0.32)",
            "0 0 24px rgba(124, 92, 255, 0.15)",
          ],
        }}
        transition={{
          duration: 5.2,
          ease: [0.42, 0, 0.58, 1],
          repeat: Infinity,
        }}
        className="text-[42px] leading-tight font-light tracking-tight"
        style={{ color: "rgba(236, 236, 241, 0.92)" }}
      >
        Hi{first ? `, ${first}` : ""}.
      </motion.div>
      <div
        className="text-[15px] font-light mt-2 tracking-wide"
        style={{ color: "rgba(236, 236, 241, 0.55)" }}
      >
        I'm here. Type or press the mic when you're ready.
      </div>
    </motion.div>
  );
}

function firstNameOf(name: string | null | undefined): string | null {
  if (!name) return null;
  const trimmed = name.trim();
  if (!trimmed) return null;
  return trimmed.split(/\s+/)[0];
}
