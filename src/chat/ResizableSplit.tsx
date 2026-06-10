/**
 * v0.20.1 — horizontal split with a draggable handle between two
 * panes. Used by Manage when the document previewer is open.
 *
 * Fraction (0..1) is the LEFT pane's share of the total width. The
 * caller owns the fraction (so it can persist + share). We just
 * report deltas on drag.
 */
import { useEffect, useRef } from "react";

interface Props {
  fraction: number;
  onFractionChange: (f: number) => void;
  /** px boundaries to clamp the handle inside. */
  minLeftPx?: number;
  minRightPx?: number;
  left: React.ReactNode;
  right: React.ReactNode;
}

export function ResizableSplit({
  fraction,
  onFractionChange,
  minLeftPx = 320,
  minRightPx = 320,
  left,
  right,
}: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const draggingRef = useRef(false);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!draggingRef.current) return;
      const el = containerRef.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const offset = e.clientX - rect.left;
      const totalWidth = rect.width;
      const clampMin = minLeftPx / totalWidth;
      const clampMax = 1 - minRightPx / totalWidth;
      const f = Math.max(clampMin, Math.min(clampMax, offset / totalWidth));
      onFractionChange(f);
    };
    const onUp = () => {
      if (draggingRef.current) {
        draggingRef.current = false;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      }
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [onFractionChange, minLeftPx, minRightPx]);

  const onHandleDown = () => {
    draggingRef.current = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  };

  return (
    <div ref={containerRef} className="flex h-full w-full overflow-hidden">
      <div className="h-full min-w-0" style={{ flexBasis: `${fraction * 100}%`, flexGrow: 0, flexShrink: 0 }}>
        {left}
      </div>
      <div
        role="separator"
        aria-orientation="vertical"
        onMouseDown={onHandleDown}
        className="w-1 shrink-0 bg-white/[0.04] hover:bg-pulse/[0.50] cursor-col-resize transition-colors"
        title="Drag to resize"
      />
      <div className="h-full flex-1 min-w-0">{right}</div>
    </div>
  );
}
