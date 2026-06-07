import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { type ParsedStep, formatDuration } from "../lib/steps";

interface Props {
  step: ParsedStep;
  /// Child steps (parent_step_id === this.id) to render nested under.
  children?: ParsedStep[];
}

/// One Claude-style named substep. Compact by default — name + status +
/// duration. Tap to expand and see notes and any error detail.
export function StepRow({ step, children = [] }: Props) {
  const [expanded, setExpanded] = useState(false);
  const hasNotes = step.notes.length > 0;
  const hasChildren = children.length > 0;
  const expandable = hasNotes || hasChildren || step.status === "failed";

  const icon = (() => {
    switch (step.status) {
      case "ok":
        return <span className="text-pulse-2">✓</span>;
      case "failed":
        return <span className="text-warn">✕</span>;
      case "cancelled":
        return <span className="text-bone-3">○</span>;
      case "running":
      default:
        return (
          <span className="relative inline-flex">
            <span className="h-1.5 w-1.5 rounded-full bg-pulse-2" />
            <span className="absolute inset-0 h-1.5 w-1.5 rounded-full bg-pulse-2 animate-ping opacity-60" />
          </span>
        );
    }
  })();

  return (
    <div className="text-[11px]">
      <button
        onClick={() => expandable && setExpanded((p) => !p)}
        disabled={!expandable}
        className={
          "flex items-start gap-2 w-full text-left py-0.5 px-1 -mx-1 rounded transition-colors " +
          (expandable ? "hover:bg-white/[0.03]" : "cursor-default")
        }
      >
        <span className="shrink-0 w-3 inline-flex items-center justify-center pt-1">
          {icon}
        </span>
        <span className="flex-1 min-w-0">
          <span className="text-bone-2">{step.name}</span>
          {step.detail && (
            <span className="text-bone-3 ml-1.5 font-mono opacity-80">
              · {step.detail}
            </span>
          )}
        </span>
        <span className="shrink-0 text-bone-3 font-mono opacity-60">
          {formatDuration(step.durationMs)}
        </span>
      </button>

      <AnimatePresence>
        {expanded && expandable && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
            className="ml-6 pl-3 border-l border-pulse-2/15 mt-1"
          >
            {step.notes.map((n, i) => (
              <div
                key={i}
                className="text-bone-3 leading-relaxed py-0.5"
              >
                <span className="text-bone-3/60 mr-1">›</span>
                {n}
              </div>
            ))}
            {step.status === "failed" && step.summary && (
              <div className="text-warn font-mono py-0.5 leading-relaxed">
                {step.summary}
              </div>
            )}
            {hasChildren && (
              <div className="mt-1 space-y-0.5">
                {children.map((c) => (
                  <StepRow key={c.id} step={c} />
                ))}
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
