/**
 * IdleCanvas — v2 Shell 17.
 *
 * Splash-style greeting when there's no active conversation and either
 * (a) it's the first moment of the session (cold boot or 24h+), or
 * (b) the user has been inactive for 10min+. Any interaction (key,
 * click, mic) fires markUserActivity and useCanvasMode flips to chat.
 */
import { motion } from "framer-motion";
import { OpeningGreeting } from "../OpeningGreeting";

export function IdleCanvas() {
  return (
    <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        transition={{ duration: 0.5, ease: [0.22, 1, 0.36, 1] }}
      >
        <OpeningGreeting />
      </motion.div>
    </div>
  );
}
