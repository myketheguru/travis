/**
 * ListCard — rich list of items with per-row actions.
 *
 * The wedge that makes sidebar tabs (Tasks, Reminders, Entities,
 * Saved places, etc.) migrate into conversational cards. When the
 * user asks 'show me my open tasks', the LLM emits a `list` part;
 * this component renders it as a card instead of a placeholder.
 *
 * Each row has: label, optional meta, and up to N actions.
 * Row actions call back to the LLM via the composer bridge — no
 * dedicated wire-up per action kind.
 */
import { motion, AnimatePresence } from "framer-motion";
import type { ListRow, RowAction } from "../../lib/richResponse";
import { useAppStore } from "../../stores/app";

interface Props {
  title: string;
  rows: ListRow[];
  narration?: string;
}

export function ListCard({ title, rows, narration }: Props) {
  const setPendingComposerText = useAppStore((s) => s.setPendingComposerText);

  function handleAction(row: ListRow, action: RowAction) {
    // Bridge to Travis via the composer. The LLM interprets the verb
    // + row context and takes the action. No per-verb wiring.
    setPendingComposerText(
      `${action.verb}: ${row.label}${row.meta ? ` (${row.meta})` : ""}`,
    );
  }

  return (
    <motion.div
      layout
      className="rounded-2xl overflow-hidden"
      transition={{ layout: { duration: 0.32, ease: [0.22, 1, 0.36, 1] } }}
      style={{
        border: "1px solid rgba(255, 255, 255, 0.10)",
        background:
          "linear-gradient(180deg, rgba(255,255,255,0.03), rgba(255,255,255,0.01))",
        boxShadow: "0 4px 24px -12px rgba(0, 0, 0, 0.5)",
      }}
    >
      {/* Header */}
      <div className="px-4 py-3">
        <div
          className="text-[10px] tracking-[0.18em] uppercase font-mono mb-1"
          style={{ color: "rgba(236, 236, 241, 0.5)" }}
        >
          // list · {rows.length} {rows.length === 1 ? "item" : "items"}
        </div>
        <div
          className="text-[15px] font-medium"
          style={{ color: "rgb(236, 236, 241)" }}
        >
          {title}
        </div>
        {narration && (
          <div
            className="text-[11.5px] mt-1 leading-relaxed"
            style={{ color: "rgba(236, 236, 241, 0.6)" }}
          >
            {narration}
          </div>
        )}
      </div>

      {/* Rows */}
      {rows.length === 0 ? (
        <div
          className="px-4 py-4 text-[12px] font-mono opacity-60"
          style={{ color: "rgba(236, 236, 241, 0.7)" }}
        >
          Nothing here.
        </div>
      ) : (
        <div
          className="flex flex-col divide-y max-h-[440px] overflow-y-auto"
          style={{ borderColor: "rgba(255,255,255,0.06)" }}
        >
          <AnimatePresence initial={false}>
            {rows.map((row, i) => (
              <motion.div
                key={row.id}
                layout
                initial={{ opacity: 0, y: 4 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: 4 }}
                transition={{
                  duration: 0.24,
                  ease: [0.22, 1, 0.36, 1],
                  delay: Math.min(i * 0.02, 0.12),
                }}
                className="px-4 py-2.5 flex items-center justify-between gap-3 hover:bg-white/[0.02] transition-colors"
                style={{ borderTop: i === 0 ? "1px solid rgba(255,255,255,0.06)" : undefined }}
              >
                <div className="min-w-0 flex-1">
                  <div
                    className="text-[13px] leading-snug truncate"
                    style={{ color: "rgba(236, 236, 241, 0.92)" }}
                  >
                    {row.label}
                  </div>
                  {row.meta && (
                    <div
                      className="text-[10.5px] font-mono opacity-60 mt-0.5 truncate"
                      style={{ color: "rgba(236, 236, 241, 0.7)" }}
                    >
                      {row.meta}
                    </div>
                  )}
                </div>
                {row.actions && row.actions.length > 0 && (
                  <div className="shrink-0 flex items-center gap-1">
                    {row.actions.map((action) => (
                      <RowActionButton
                        key={action.verb}
                        action={action}
                        onClick={() => handleAction(row, action)}
                      />
                    ))}
                  </div>
                )}
              </motion.div>
            ))}
          </AnimatePresence>
        </div>
      )}
    </motion.div>
  );
}

function RowActionButton({
  action,
  onClick,
}: {
  action: RowAction;
  onClick: () => void;
}) {
  const isPrimary = action.kind === "primary";
  return (
    <motion.button
      whileHover={{ scale: 1.04 }}
      whileTap={{ scale: 0.96 }}
      onClick={onClick}
      className="text-[10px] uppercase tracking-wider font-mono px-2 py-1 rounded-md transition-colors"
      style={{
        background: isPrimary
          ? "rgba(189, 158, 255, 0.14)"
          : "rgba(255, 255, 255, 0.03)",
        color: isPrimary ? "rgb(189, 158, 255)" : "rgba(236, 236, 241, 0.85)",
        border: `1px solid ${
          isPrimary ? "rgba(189, 158, 255, 0.4)" : "rgba(255, 255, 255, 0.1)"
        }`,
      }}
    >
      {action.label}
    </motion.button>
  );
}
