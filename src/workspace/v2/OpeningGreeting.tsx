/**
 * OpeningGreeting — v2 Shell 8 + v0.27.4 orb.
 *
 * The idle-canvas splash. Renders a large ambient orb above 'Hi,
 * {first}.' greeting. The orb is a soft radial gradient with slow
 * chromatic breathing — a calm signal that Travis is idle and ready.
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
      className="text-center pointer-events-none select-none flex flex-col items-center gap-6"
    >
      <motion.div
        aria-hidden
        animate={{
          scale: [1, 1.06, 1],
          opacity: [0.85, 1, 0.85],
        }}
        transition={{
          duration: 5.6,
          repeat: Infinity,
          ease: [0.42, 0, 0.58, 1],
        }}
        className="rounded-full"
        style={{
          width: 168,
          height: 168,
          background:
            "radial-gradient(circle at 32% 28%, rgba(236,236,241,0.92) 0%, rgba(189,158,255,0.6) 42%, rgba(124,92,255,0.25) 72%, rgba(0,0,0,0) 100%)",
          boxShadow:
            "0 0 60px rgba(189, 158, 255, 0.35), 0 0 120px rgba(124, 92, 255, 0.25)",
        }}
      />

      <div>
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
