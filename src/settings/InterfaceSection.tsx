/**
 * InterfaceSection — v2 vs classic surface toggle.
 *
 * v0.25 introduces the v2 canvas + HUD workspace. Both surfaces coexist
 * during rollout; users can flip between them here.
 */
import { motion } from "framer-motion";
import { useAppStore } from "../stores/app";

export function InterfaceSection() {
  const surface = useAppStore((s) => s.uiSurface);
  const setSurface = useAppStore((s) => s.setUiSurface);

  return (
    <div className="flex flex-col gap-3">
      <p className="text-bone-3 text-[11px] leading-relaxed">
        Travis has a new canvas + HUD interface. It's the future default; if
        you prefer the classic layout, flip below.
      </p>
      <div className="grid grid-cols-2 gap-2">
        <SurfaceOption
          value="v2"
          current={surface}
          setSurface={setSurface}
          title="Canvas (v2)"
          subtitle="HUD overlays, focal cards, video-game feel"
        />
        <SurfaceOption
          value="classic"
          current={surface}
          setSurface={setSurface}
          title="Classic"
          subtitle="Chat surface wrapped in the shell"
        />
      </div>
    </div>
  );
}

function SurfaceOption({
  value,
  current,
  setSurface,
  title,
  subtitle,
}: {
  value: "v2" | "classic";
  current: "v2" | "classic";
  setSurface: (s: "v2" | "classic") => void;
  title: string;
  subtitle: string;
}) {
  const active = current === value;
  return (
    <motion.button
      whileHover={{ scale: 1.02 }}
      whileTap={{ scale: 0.98 }}
      onClick={() => setSurface(value)}
      className="text-left px-3 py-2.5 rounded-md transition-colors"
      style={{
        background: active
          ? "rgba(189, 158, 255, 0.10)"
          : "rgba(255, 255, 255, 0.02)",
        border: `1px solid ${
          active ? "rgba(189, 158, 255, 0.4)" : "rgba(255, 255, 255, 0.08)"
        }`,
      }}
    >
      <div
        className="text-[12.5px] font-medium"
        style={{
          color: active ? "rgb(189, 158, 255)" : "rgba(236, 236, 241, 0.9)",
        }}
      >
        {title}
      </div>
      <div
        className="text-[10.5px] font-mono opacity-60 mt-0.5"
        style={{ color: "rgba(236, 236, 241, 0.7)" }}
      >
        {subtitle}
      </div>
    </motion.button>
  );
}
