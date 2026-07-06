/**
 * AttentionCompass — v0.27 (v2 Shell 14 cleanup).
 *
 * Compact TR chip that shows the count of attention items, expanding
 * to a small popover on click. Replaces the big AttentionStrip block
 * that used to hog peripheral space in the v2 canvas.
 */
import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useAttentionItems, type AttentionItem } from "../useAttentionItems";

export function AttentionCompass() {
  const [open, setOpen] = useState(false);
  const { items, loading } = useAttentionItems();

  const count = items.length;
  const label = count === 0 ? "all clear" : `${count}`;
  const tint =
    count === 0 ? "rgba(129, 199, 132, 0.85)" : "rgba(255, 179, 92, 0.90)";

  return (
    <div className="relative">
      <motion.button
        whileHover={{ scale: 1.04 }}
        whileTap={{ scale: 0.96 }}
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-2 px-3 py-1.5 rounded-full backdrop-blur-md"
        style={{
          background: "rgba(0, 0, 0, 0.35)",
          border: "1px solid rgba(255, 255, 255, 0.10)",
        }}
        aria-label={`Attention · ${label}`}
        title="Attention"
      >
        <span
          className="text-[9px] uppercase tracking-[0.24em] font-mono"
          style={{ color: "rgba(236, 236, 241, 0.55)" }}
        >
          attention
        </span>
        <span
          className="w-1.5 h-1.5 rounded-full"
          style={{ background: tint, boxShadow: `0 0 8px ${tint}` }}
        />
        <span
          className="text-[11px] font-mono"
          style={{ color: "rgba(236, 236, 241, 0.85)" }}
        >
          {loading && count === 0 ? "…" : label}
        </span>
      </motion.button>

      <AnimatePresence>
        {open && count > 0 && (
          <motion.div
            key="popover"
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
            className="absolute right-0 mt-2 w-[280px] rounded-2xl backdrop-blur-md p-2"
            style={{
              background: "rgba(0, 0, 0, 0.55)",
              border: "1px solid rgba(255, 255, 255, 0.10)",
            }}
          >
            <div className="flex flex-col gap-1 max-h-[300px] overflow-y-auto">
              {items.map((it) => (
                <CompassItem key={it.id} item={it} />
              ))}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function CompassItem({ item }: { item: AttentionItem }) {
  return (
    <div
      className="rounded-xl px-3 py-2"
      style={{ background: "rgba(255, 255, 255, 0.04)" }}
    >
      <div
        className="text-[9px] uppercase tracking-wider font-mono"
        style={{ color: "rgba(236, 236, 241, 0.5)" }}
      >
        {item.kind ?? "item"}
      </div>
      <div
        className="text-[13px] mt-0.5 leading-snug"
        style={{ color: "rgba(236, 236, 241, 0.9)" }}
      >
        {item.label}
      </div>
    </div>
  );
}
