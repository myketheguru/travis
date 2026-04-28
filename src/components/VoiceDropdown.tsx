import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { VOICE_PRESETS, presetFromDescription } from "../onboarding/voicePresets";

/**
 * Custom dropdown for picking a communication-style preset. Mirrors a
 * native <select>'s behaviour (click outside to close, Escape to close,
 * keyboard navigation) but matches the app's dark/glassy aesthetic and
 * shows each preset's blurb under its label so the user can tell them
 * apart at a glance.
 *
 * Stores the preset's `description` string verbatim in the value (which
 * is what gets injected into prompts). When the user picks "Custom" a
 * free-form input appears below the trigger.
 */
export function VoiceDropdown({
  value,
  onChange,
}: {
  value: string;
  onChange: (next: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [customMode, setCustomMode] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const customInputRef = useRef<HTMLInputElement>(null);

  // Resolve the current value to a preset (or null if it's custom text).
  const matchedPreset = presetFromDescription(value);
  const isCustom =
    customMode || (!matchedPreset && value.trim().length > 0);
  const activeId = isCustom ? "custom" : matchedPreset?.id ?? "default";

  // Sync customMode flag when the prop value changes externally.
  useEffect(() => {
    if (matchedPreset && !customMode) return;
    if (!matchedPreset && value.trim().length > 0) {
      setCustomMode(true);
    }
  }, [value, matchedPreset, customMode]);

  // Click outside → close.
  useEffect(() => {
    if (!open) return;
    const onDocMouseDown = (e: MouseEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDocMouseDown);
    return () => document.removeEventListener("mousedown", onDocMouseDown);
  }, [open]);

  // Escape → close.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open]);

  const triggerLabel = isCustom
    ? "Custom"
    : matchedPreset?.label ?? "Default";
  const triggerBlurb = isCustom
    ? value.trim() || "Write your own voice instructions."
    : matchedPreset?.blurb ?? "";

  const pickPreset = (id: string, description: string) => {
    if (id === "custom") {
      setCustomMode(true);
      // Defer so the input renders before we focus it.
      setTimeout(() => customInputRef.current?.focus(), 30);
    } else {
      setCustomMode(false);
      onChange(description);
    }
    setOpen(false);
  };

  return (
    <div ref={containerRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="listbox"
        aria-expanded={open}
        className={
          "w-full text-left rounded-xl border bg-ink-2/40 px-4 py-3 transition-colors flex items-center gap-3 " +
          (open
            ? "border-pulse/60 bg-pulse/[0.06]"
            : "border-ink-3 hover:border-ink-3/80 hover:bg-ink-2/60")
        }
      >
        <span className="flex-1 min-w-0">
          <span className="block text-bone font-medium text-sm leading-snug">
            {triggerLabel}
          </span>
          <span className="block text-bone-3 text-[11px] mt-0.5 leading-snug truncate">
            {triggerBlurb}
          </span>
        </span>
        <motion.span
          aria-hidden
          animate={{ rotate: open ? 180 : 0 }}
          transition={{ duration: 0.2 }}
          className="text-bone-3 text-xs flex-shrink-0"
        >
          ▾
        </motion.span>
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            role="listbox"
            initial={{ opacity: 0, y: -6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -6 }}
            transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
            className="absolute left-0 right-0 top-[calc(100%+6px)] z-30 rounded-xl border border-ink-3 bg-ink-2/95 backdrop-blur-md p-1.5 flex flex-col gap-0.5 max-h-[60vh] overflow-y-auto"
            style={{
              boxShadow:
                "0 18px 48px -16px rgba(0,0,0,0.6), 0 4px 14px -4px rgba(124,92,255,0.18)",
            }}
          >
            {VOICE_PRESETS.map((p) => {
              const active = activeId === p.id;
              return (
                <button
                  key={p.id}
                  type="button"
                  role="option"
                  aria-selected={active}
                  onClick={() => pickPreset(p.id, p.description)}
                  className={
                    "text-left rounded-lg px-3 py-2 transition-colors flex items-start gap-3 " +
                    (active
                      ? "bg-pulse/[0.10]"
                      : "hover:bg-white/[0.04]")
                  }
                >
                  <span className="flex-1 min-w-0">
                    <span className="block text-bone text-sm font-medium leading-snug">
                      {p.label}
                    </span>
                    <span className="block text-bone-3 text-[11px] mt-0.5 leading-snug">
                      {p.blurb}
                    </span>
                  </span>
                  {active && (
                    <span className="mt-1 h-1.5 w-1.5 rounded-full bg-pulse-2 shadow-[0_0_8px_rgba(110,196,232,0.7)] flex-shrink-0" />
                  )}
                </button>
              );
            })}

            <div className="h-px bg-white/[0.06] my-1" />

            <button
              type="button"
              role="option"
              aria-selected={isCustom}
              onClick={() => pickPreset("custom", "")}
              className={
                "text-left rounded-lg px-3 py-2 transition-colors flex items-start gap-3 " +
                (isCustom ? "bg-pulse/[0.10]" : "hover:bg-white/[0.04]")
              }
            >
              <span className="flex-1 min-w-0">
                <span className="block text-bone text-sm font-medium leading-snug">
                  Custom
                </span>
                <span className="block text-bone-3 text-[11px] mt-0.5 leading-snug">
                  Write your own voice instructions.
                </span>
              </span>
              {isCustom && (
                <span className="mt-1 h-1.5 w-1.5 rounded-full bg-pulse-2 shadow-[0_0_8px_rgba(110,196,232,0.7)] flex-shrink-0" />
              )}
            </button>
          </motion.div>
        )}
      </AnimatePresence>

      {isCustom && !open && (
        <input
          ref={customInputRef}
          value={value}
          placeholder="e.g. blunt, no preamble, action verbs only"
          onChange={(e) => onChange(e.target.value)}
          className="mt-2 w-full bg-ink-2/70 border border-ink-3 rounded-lg px-3.5 py-2.5 text-bone placeholder:text-bone-3/55 focus:outline-none focus:border-pulse/60 transition-colors"
        />
      )}
    </div>
  );
}
