import { useEffect, useState } from "react";
import { motion, useReducedMotion } from "framer-motion";
import { useAppStore, type Activity } from "../stores/app";

type Profile = {
  speed: number;
  spread: number;
  saturation: number;
  reactOpacity: number;
};

const profiles: Record<Activity, Profile> = {
  idle:      { speed: 1.00, spread: 1.00, saturation: 1.05, reactOpacity: 1.00 },
  typing:    { speed: 1.45, spread: 1.06, saturation: 1.18, reactOpacity: 1.10 },
  thinking:  { speed: 1.75, spread: 1.10, saturation: 1.28, reactOpacity: 1.18 },
  listening: { speed: 1.65, spread: 1.20, saturation: 1.35, reactOpacity: 1.24 },
  speaking:  { speed: 1.30, spread: 1.10, saturation: 1.22, reactOpacity: 1.14 },
};

type Blob = {
  color: string;
  size: string;
  durX: number;
  durY: number;
  dx: number;
  dy: number;
  phase: number;
  hueRotateDur: number;
};

const blobs: Blob[] = [
  { color: "rgba(190,118,255,0.78)", size: "82%", durX: 12.4, durY: 9.8,  dx: 38, dy: 30, phase: 0.00, hueRotateDur: 42 },
  { color: "rgba(255,108,196,0.72)", size: "74%", durX: 14.6, durY: 11.2, dx: 32, dy: 38, phase: 0.55, hueRotateDur: 56 },
  { color: "rgba(94,138,255,0.78)",  size: "86%", durX: 16.0, durY: 10.6, dx: 28, dy: 42, phase: 0.95, hueRotateDur: 48 },
  { color: "rgba(72,212,255,0.70)",  size: "70%", durX: 10.8, durY: 14.0, dx: 44, dy: 32, phase: 0.25, hueRotateDur: 64 },
  { color: "rgba(255,178,108,0.55)", size: "58%", durX: 17.2, durY: 12.8, dx: 24, dy: 22, phase: 0.80, hueRotateDur: 72 },
];

export function PresenceOrb({ size = 240 }: { size?: number }) {
  const activity = useAppStore((s) => s.activity);
  const reduce = useReducedMotion();
  const p = profiles[activity];

  const [listenTick, setListenTick] = useState(0);
  const [cleanTick, setCleanTick] = useState(0);

  useEffect(() => {
    if (reduce) return;
    let id: ReturnType<typeof setTimeout>;
    const schedule = () => {
      const delay = 4500 + Math.random() * 4500;
      id = setTimeout(() => {
        setListenTick((t) => t + 1);
        schedule();
      }, delay);
    };
    schedule();
    return () => clearTimeout(id);
  }, [reduce]);

  useEffect(() => {
    if (reduce) return;
    let id: ReturnType<typeof setTimeout>;
    const schedule = () => {
      const delay = 5500 + Math.random() * 5000;
      id = setTimeout(() => {
        setCleanTick((t) => t + 1);
        schedule();
      }, delay);
    };
    schedule();
    return () => clearTimeout(id);
  }, [reduce]);

  return (
    <div
      className="relative pointer-events-none select-none"
      style={{ width: size, height: size }}
      aria-hidden
    >
      <motion.div
        className="absolute -inset-[28%] rounded-full"
        style={{
          background:
            "radial-gradient(circle, rgba(190,118,255,0.30) 0%, rgba(72,212,255,0.16) 38%, transparent 72%)",
          filter: "blur(72px)",
        }}
        animate={
          reduce
            ? { opacity: 0.6 * p.reactOpacity }
            : {
                opacity: [0.45 * p.reactOpacity, 0.72 * p.reactOpacity, 0.45 * p.reactOpacity],
                scale: [1, 1.04, 1],
              }
        }
        transition={{ duration: 6 / p.speed, repeat: Infinity, ease: "easeInOut" }}
      />

      <motion.div
        className="absolute inset-0 rounded-full overflow-hidden"
        style={{
          filter: `saturate(${p.saturation})`,
          // WebKit (macOS) doesn't reliably clip filtered content + children
          // with mix-blend-mode to a rounded `overflow:hidden` parent. The
          // blobs below would render as a rectangular bounding box. clip-path
          // is composited correctly through the filter pipeline; isolation
          // pins the blend modes to this stacking context. Together they
          // restore the circular silhouette on macOS.
          clipPath: "circle(50%)",
          WebkitClipPath: "circle(50%)",
          isolation: "isolate",
        }}
        animate={
          reduce
            ? {}
            : {
                filter: [
                  `saturate(${p.saturation}) hue-rotate(0deg)`,
                  `saturate(${p.saturation}) hue-rotate(360deg)`,
                ],
              }
        }
        transition={{ duration: 60, repeat: Infinity, ease: "linear" }}
      >
        <div
          className="absolute inset-0"
          style={{
            background:
              "radial-gradient(circle at 50% 55%, #1a1230 0%, #0a0a18 65%, #07080b 100%)",
          }}
        />

        {blobs.map((b, i) => (
          <motion.div
            key={i}
            className="absolute rounded-full"
            style={{
              width: b.size,
              height: b.size,
              left: `calc(50% - ${b.size} / 2)`,
              top: `calc(50% - ${b.size} / 2)`,
              background: `radial-gradient(circle, ${b.color} 0%, transparent 68%)`,
              mixBlendMode: "screen",
              filter: "blur(30px)",
              willChange: "transform, filter",
            }}
            animate={
              reduce
                ? {}
                : {
                    x: [-b.dx * p.spread, b.dx * p.spread, -b.dx * p.spread],
                    y: [b.dy * p.spread, -b.dy * p.spread, b.dy * p.spread],
                    filter: [
                      `blur(30px) hue-rotate(0deg)`,
                      `blur(30px) hue-rotate(360deg)`,
                    ],
                  }
            }
            transition={{
              x: {
                duration: b.durX / p.speed,
                repeat: Infinity,
                ease: "easeInOut",
                delay: b.phase,
              },
              y: {
                duration: b.durY / p.speed,
                repeat: Infinity,
                ease: "easeInOut",
                delay: b.phase * 1.3,
              },
              filter: {
                duration: b.hueRotateDur,
                repeat: Infinity,
                ease: "linear",
              },
            }}
          />
        ))}

        <motion.div
          className="absolute inset-0 rounded-full"
          style={{
            background:
              "conic-gradient(from 0deg, rgba(190,118,255,0.35), rgba(72,212,255,0.30), rgba(255,108,196,0.32), rgba(94,138,255,0.30), rgba(190,118,255,0.35))",
            mixBlendMode: "soft-light",
            filter: "blur(26px)",
            willChange: "transform",
          }}
          animate={reduce ? {} : { rotate: 360 }}
          transition={{ duration: 32 / p.speed, repeat: Infinity, ease: "linear" }}
        />

        <motion.div
          className="absolute rounded-full"
          style={{
            width: "26%",
            height: "26%",
            left: "37%",
            top: "37%",
            background:
              "radial-gradient(circle at 40% 40%, rgba(255,255,255,0.55) 0%, rgba(255,255,255,0.18) 40%, rgba(255,255,255,0) 70%)",
            filter: "blur(8px)",
            mixBlendMode: "screen",
            willChange: "transform, opacity",
          }}
          animate={
            reduce
              ? { opacity: 0.5 }
              : {
                  x: [0, 24, 8, -22, -10, 0],
                  y: [-22, -8, 18, 6, -16, -22],
                  scale: [1, 1.06, 0.96, 1.08, 0.98, 1],
                  opacity: [0.45, 0.65, 0.4, 0.7, 0.5, 0.45],
                }
          }
          transition={{
            duration: 16 / p.speed,
            repeat: Infinity,
            ease: "easeInOut",
          }}
        />

        <motion.div
          key={`listen-${listenTick}`}
          className="absolute inset-0 rounded-full"
          style={{
            background:
              "radial-gradient(circle at 50% 50%, rgba(255,255,255,0.18) 0%, rgba(255,255,255,0) 55%)",
            mixBlendMode: "screen",
          }}
          initial={{ scale: 0.92, opacity: 0 }}
          animate={{ scale: [0.92, 1.05, 1.12], opacity: [0, 0.5, 0] }}
          transition={{ duration: 1.6, ease: "easeOut" }}
        />

        <motion.div
          key={`clean-${cleanTick}`}
          className="absolute inset-0 rounded-full"
          style={{
            background:
              "linear-gradient(115deg, transparent 38%, rgba(255,255,255,0.40) 50%, transparent 62%)",
            mixBlendMode: "screen",
            filter: "blur(6px)",
          }}
          initial={{ x: "-65%", opacity: 0 }}
          animate={{ x: ["-65%", "65%"], opacity: [0, 0.8, 0] }}
          transition={{ duration: 1.4, ease: [0.4, 0, 0.6, 1] }}
        />

        <motion.div
          key={`activity-${activity}`}
          className="absolute inset-0 rounded-full"
          style={{
            background:
              "radial-gradient(circle at 50% 50%, rgba(255,255,255,0.25) 0%, rgba(255,255,255,0) 55%)",
            mixBlendMode: "screen",
          }}
          initial={{ scale: 1, opacity: 0 }}
          animate={
            activity === "idle"
              ? { scale: 1, opacity: 0 }
              : { scale: [1, 1.16, 1.28], opacity: [0.55, 0.25, 0] }
          }
          transition={{ duration: 0.9, ease: "easeOut" }}
        />

        <div
          className="absolute inset-0 rounded-full"
          style={{
            boxShadow:
              "inset 0 0 0 1px rgba(255,255,255,0.05), inset 0 1px 22px rgba(255,255,255,0.07), inset 0 -10px 30px rgba(20,10,40,0.42)",
          }}
        />
      </motion.div>

      <motion.div
        className="absolute inset-0 rounded-full"
        style={{
          boxShadow:
            "0 0 60px 5px rgba(190,118,255,0.20), 0 0 120px 12px rgba(72,212,255,0.10)",
        }}
        animate={
          reduce
            ? { opacity: 0.7 }
            : { opacity: [0.55, 0.85, 0.55] }
        }
        transition={{ duration: 5.5 / p.speed, repeat: Infinity, ease: "easeInOut" }}
      />
    </div>
  );
}
