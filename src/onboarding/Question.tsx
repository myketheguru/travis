import { motion } from "framer-motion";
import type { ReactNode } from "react";

const enter = {
  initial: { opacity: 0, y: 28 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -28 },
  transition: { duration: 0.45, ease: [0.16, 1, 0.3, 1] as [number, number, number, number] },
};

export function Question({
  index,
  prompt,
  hint,
  children,
  canAdvance,
  onAdvance,
  advanceLabel = "Continue",
  optional,
  onSkip,
}: {
  index: number;
  prompt: string;
  hint?: string;
  children: ReactNode;
  canAdvance: boolean;
  onAdvance: () => void;
  advanceLabel?: string;
  optional?: boolean;
  onSkip?: () => void;
}) {
  return (
    <motion.div
      key={index}
      {...enter}
      className="flex flex-col gap-6"
      onKeyDown={(e) => {
        if (e.key === "Enter" && canAdvance) {
          e.preventDefault();
          onAdvance();
        }
      }}
    >
      <div className="flex items-baseline gap-3">
        <span className="text-pulse-2/80 text-xs font-mono tracking-wider">
          {String(index).padStart(2, "0")}
        </span>
        <h2 className="text-2xl md:text-3xl font-light tracking-tight text-bone leading-snug">
          {prompt}
        </h2>
      </div>
      {hint && (
        <p className="text-bone-3 text-xs -mt-3 ml-9 max-w-md leading-relaxed">
          {hint}
        </p>
      )}

      <div className="ml-9">{children}</div>

      <div className="ml-9 mt-2 flex items-center gap-4">
        <button
          onClick={onAdvance}
          disabled={!canAdvance}
          className="px-5 py-2.5 rounded-full bg-bone/95 text-ink text-sm font-medium disabled:opacity-25 disabled:cursor-not-allowed hover:bg-bone transition-all min-w-[110px]"
        >
          {advanceLabel}
        </button>
        <span className="text-bone-3 text-[11px] tracking-wider">
          press <kbd className="px-1.5 py-0.5 rounded border border-ink-3 bg-ink-2/60 text-bone-2 font-mono text-[10px]">Enter</kbd>
        </span>
        {optional && onSkip && (
          <button
            onClick={onSkip}
            className="ml-auto text-bone-3 hover:text-bone-2 text-xs underline-offset-4 hover:underline transition-colors"
          >
            Skip
          </button>
        )}
      </div>
    </motion.div>
  );
}

export const inputClass =
  "w-full bg-transparent border-b border-ink-3 focus:border-pulse/70 px-1 py-2.5 text-bone text-xl font-light placeholder:text-bone-3/40 focus:outline-none transition-colors";
