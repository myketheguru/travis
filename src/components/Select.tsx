/**
 * v0.20.1 — themeable Select component.
 *
 * Replaces native `<select>` everywhere it's leaking through (which
 * looks unstyled on Windows + macOS). Same controlled-input contract:
 * { value, onChange, options }. No portal complexity — the dropdown
 * is positioned absolute under the trigger; closes on outside click
 * or Escape.
 *
 * Accessibility: trigger is a button, dropdown is role="listbox",
 * options are role="option". Arrow up/down + Enter navigation.
 */
import { useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";

export interface SelectOption {
  value: string;
  label?: string;
  /** Optional disabled state. */
  disabled?: boolean;
}

interface Props {
  value: string;
  onChange: (next: string) => void;
  options: SelectOption[];
  /** Optional className applied to the trigger button. */
  className?: string;
  /** Optional placeholder shown when value isn't in the option set. */
  placeholder?: string;
  /** Optional pixel width override for the popover. */
  width?: number | string;
  /** Optional title attribute for the trigger button (tooltip). */
  title?: string;
  /** Optional aria-label. */
  "aria-label"?: string;
}

export function Select({
  value,
  onChange,
  options,
  className,
  placeholder,
  width,
  title,
  "aria-label": ariaLabel,
}: Props) {
  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState<number>(-1);
  const wrapperRef = useRef<HTMLDivElement | null>(null);

  const currentLabel = (() => {
    const match = options.find((o) => o.value === value);
    return match?.label ?? match?.value ?? placeholder ?? value;
  })();

  // Outside click + Escape close
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setHighlight((h) => Math.min(options.length - 1, h + 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setHighlight((h) => Math.max(0, h - 1));
      } else if (e.key === "Enter" && highlight >= 0) {
        e.preventDefault();
        const opt = options[highlight];
        if (opt && !opt.disabled) {
          onChange(opt.value);
          setOpen(false);
        }
      }
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open, options, highlight, onChange]);

  // Reset highlight to current value when opening
  useEffect(() => {
    if (open) {
      const idx = options.findIndex((o) => o.value === value);
      setHighlight(idx >= 0 ? idx : 0);
    }
  }, [open, options, value]);

  return (
    <div ref={wrapperRef} className="relative inline-block">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        title={title}
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        className={
          "inline-flex items-center gap-1 px-2 py-0.5 rounded text-bone-2 bg-white/[0.04] hover:bg-white/[0.07] transition-colors text-[11px] font-mono " +
          (className ?? "")
        }
      >
        <span className="truncate">{currentLabel}</span>
        <svg
          viewBox="0 0 24 24"
          width="9"
          height="9"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.8"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
          className="opacity-60 shrink-0"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>
      <AnimatePresence>
        {open && (
          <motion.div
            role="listbox"
            initial={{ opacity: 0, y: -2 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -2 }}
            transition={{ duration: 0.1 }}
            style={{ width }}
            className="absolute z-30 top-full mt-1 left-0 min-w-[140px] max-h-[280px] overflow-y-auto rounded-md bg-[#0c0d11] border border-white/[0.08] shadow-xl py-1"
          >
            {options.map((opt, idx) => {
              const isActive = opt.value === value;
              const isHighlighted = idx === highlight;
              return (
                <button
                  key={opt.value}
                  role="option"
                  aria-selected={isActive}
                  disabled={opt.disabled}
                  onMouseEnter={() => setHighlight(idx)}
                  onClick={() => {
                    if (opt.disabled) return;
                    onChange(opt.value);
                    setOpen(false);
                  }}
                  className={
                    "w-full text-left px-3 py-1 text-[11px] font-mono flex items-center justify-between transition-colors " +
                    (opt.disabled
                      ? "text-bone-3/40 cursor-not-allowed"
                      : isHighlighted
                      ? "bg-white/[0.06] text-bone"
                      : "text-bone-2 hover:bg-white/[0.04]")
                  }
                >
                  <span className="truncate">{opt.label ?? opt.value}</span>
                  {isActive && (
                    <svg
                      viewBox="0 0 24 24"
                      width="10"
                      height="10"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      aria-hidden
                      className="opacity-80 shrink-0"
                    >
                      <polyline points="20 6 9 17 4 12" />
                    </svg>
                  )}
                </button>
              );
            })}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
