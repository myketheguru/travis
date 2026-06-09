import { useCallback, useEffect, useRef, useState } from "react";

/// Returns `atBottom` (within `threshold` px of the scroll container's
/// bottom edge) and `scrollToBottom`. Use the ref returned by the hook
/// on the scrollable element.
///
/// Behaviour:
/// - First load: jumps to the bottom once (after the first non-empty
///   render).
/// - Subsequent content changes: only scrolls if the user was already at
///   the bottom before the change. Lets the user scroll up and read
///   without being yanked back down.
/// - The caller can force a scroll via the returned `scrollToBottom`.
export function useScrollAnchor<T>(deps: T, threshold = 80) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [atBottom, setAtBottom] = useState(true);
  const atBottomRef = useRef(true);
  const didFirstScroll = useRef(false);

  const measure = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    const distanceFromBottom =
      el.scrollHeight - el.scrollTop - el.clientHeight;
    const next = distanceFromBottom <= threshold;
    atBottomRef.current = next;
    setAtBottom(next);
  }, [threshold]);

  const scrollToBottom = useCallback((behavior: ScrollBehavior = "smooth") => {
    const el = ref.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior });
    atBottomRef.current = true;
    setAtBottom(true);
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onScroll = () => measure();
    el.addEventListener("scroll", onScroll, { passive: true });
    measure();
    return () => {
      el.removeEventListener("scroll", onScroll);
    };
  }, [measure]);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (!didFirstScroll.current) {
      // Jump on first paint where deps are real (non-empty), then mark
      // the first-scroll guard so subsequent re-mounts don't snap.
      // v0.18.2 — pairs with the chunked-history loader: only the
      // most recent 50 messages are in `messages` on first paint, so
      // scrolling to bottom shows them naturally, and the user can
      // scroll up to fetch older chunks.
      if (el.scrollHeight > el.clientHeight) {
        el.scrollTo({ top: el.scrollHeight, behavior: "auto" });
        atBottomRef.current = true;
        setAtBottom(true);
        didFirstScroll.current = true;
      }
      return;
    }
    if (atBottomRef.current) {
      el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
    }
    // deps drives this effect — explicit, not in the dep array, because
    // T is opaque and we just want any change to retrigger the check.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [deps]);

  return { ref, atBottom, scrollToBottom };
}
