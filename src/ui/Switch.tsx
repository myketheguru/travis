/**
 * Switch — v0.28.45 project-wide toggle control.
 *
 * Replaces every checkbox across Travis surfaces (Settings,
 * onboarding, overlays). Brand-purple track when on, dark when off,
 * white pill that slides between. Framer-motion drives the thumb so
 * the ease matches the rest of the workspace.
 *
 * Usage:
 *   <Switch checked={value} onChange={setValue} label="Do the thing" />
 *
 * Label is optional — pass it as either a prop or wrap the Switch in
 * a `<label>` yourself if you want richer layout. The component is a
 * button under the hood with `role="switch"` + `aria-checked`, and it
 * handles space/enter keyboard toggling.
 */
import { motion } from "framer-motion";

type SwitchSize = "sm" | "md";

const DIMS: Record<SwitchSize, { track: [number, number]; thumb: number; padding: number }> = {
  sm: { track: [30, 18], thumb: 14, padding: 2 },
  md: { track: [38, 22], thumb: 18, padding: 2 },
};

export function Switch({
  checked,
  onChange,
  disabled = false,
  label,
  description,
  size = "md",
  id,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  disabled?: boolean;
  label?: React.ReactNode;
  description?: React.ReactNode;
  size?: SwitchSize;
  id?: string;
}) {
  const dim = DIMS[size];
  const trackW = dim.track[0];
  const trackH = dim.track[1];
  const thumb = dim.thumb;
  const pad = dim.padding;
  const travel = trackW - thumb - pad * 2;

  const toggle = () => {
    if (disabled) return;
    onChange(!checked);
  };

  const control = (
    <button
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-disabled={disabled}
      disabled={disabled}
      onClick={toggle}
      onKeyDown={(e) => {
        if (e.key === " " || e.key === "Enter") {
          e.preventDefault();
          toggle();
        }
      }}
      className="relative inline-flex shrink-0 items-center rounded-full transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2"
      style={{
        width: trackW,
        height: trackH,
        padding: pad,
        background: checked
          ? "linear-gradient(180deg, rgba(189,158,255,0.85), rgba(140,105,235,0.85))"
          : "rgba(255,255,255,0.12)",
        border: `1px solid ${checked ? "rgba(189,158,255,0.55)" : "rgba(255,255,255,0.14)"}`,
        cursor: disabled ? "not-allowed" : "pointer",
        opacity: disabled ? 0.45 : 1,
        boxShadow: checked
          ? "0 0 12px -2px rgba(189, 158, 255, 0.55), inset 0 0 6px rgba(255,255,255,0.15)"
          : "inset 0 0 4px rgba(0,0,0,0.4)",
      }}
    >
      <motion.span
        className="block rounded-full"
        style={{
          width: thumb,
          height: thumb,
          background: "rgba(250, 248, 255, 0.98)",
          boxShadow: "0 1px 3px rgba(0,0,0,0.55), 0 0 4px rgba(189,158,255,0.5)",
        }}
        animate={{ x: checked ? travel : 0 }}
        transition={{ type: "spring", stiffness: 480, damping: 34, mass: 0.6 }}
      />
    </button>
  );

  if (!label && !description) return control;

  // With label: horizontally align the control at the top of a text
  // block so a longer description doesn't shift the switch off-center.
  return (
    <label
      className="flex items-start gap-3"
      style={{ cursor: disabled ? "not-allowed" : "pointer" }}
      onClick={(e) => {
        // Prevent double-toggle: click on the label bubbles to the
        // wrapped button naturally. If we handled it here too, the
        // switch would flip back immediately.
        if ((e.target as HTMLElement).closest("button")) return;
        e.preventDefault();
        toggle();
      }}
    >
      {control}
      <span className="min-w-0 flex-1 leading-snug">
        {label && (
          <span
            className="block text-[13.5px] font-medium"
            style={{ color: "rgba(236, 236, 241, 0.94)" }}
          >
            {label}
          </span>
        )}
        {description && (
          <span
            className="block text-[12px] mt-0.5"
            style={{ color: "rgba(236, 236, 241, 0.60)" }}
          >
            {description}
          </span>
        )}
      </span>
    </label>
  );
}
