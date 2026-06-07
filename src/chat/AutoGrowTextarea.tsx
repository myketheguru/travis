import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  type KeyboardEvent,
} from "react";

interface Props {
  value: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
  placeholder?: string;
  disabled?: boolean;
  maxRows?: number;
}

/// Auto-growing textarea that mirrors Claude/Discord/ChatGPT input
/// behavior: starts at 1 row, grows with content up to maxRows, then
/// scrolls inside. Enter submits; Shift+Enter inserts a newline.
export const AutoGrowTextarea = forwardRef<HTMLTextAreaElement, Props>(
  function AutoGrowTextarea(
    { value, onChange, onSubmit, placeholder, disabled, maxRows = 8 },
    ref,
  ) {
    const localRef = useRef<HTMLTextAreaElement | null>(null);
    useImperativeHandle(ref, () => localRef.current as HTMLTextAreaElement, []);

    const recalcHeight = () => {
      const el = localRef.current;
      if (!el) return;
      // Reset to single-line so scrollHeight measures content height
      el.style.height = "auto";
      const lineHeight =
        parseFloat(getComputedStyle(el).lineHeight || "20") || 20;
      const padding = 16; // 8px top + 8px bottom approx
      const maxHeight = lineHeight * maxRows + padding;
      const nextHeight = Math.min(el.scrollHeight, maxHeight);
      el.style.height = `${nextHeight}px`;
      el.style.overflowY = el.scrollHeight > maxHeight ? "auto" : "hidden";
    };

    useEffect(() => {
      recalcHeight();
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [value, maxRows]);

    // Autofocus on mount
    useEffect(() => {
      localRef.current?.focus();
    }, []);

    const handleKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        onSubmit();
      }
    };

    return (
      <textarea
        ref={localRef}
        rows={1}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={handleKey}
        placeholder={placeholder}
        disabled={disabled}
        className="flex-1 bg-transparent px-1 py-2 text-bone text-[15px] font-light placeholder:text-bone-3/50 focus:outline-none disabled:text-bone-2/70 resize-none leading-relaxed"
        style={{ minHeight: "36px", whiteSpace: "pre-wrap", wordBreak: "break-word" }}
      />
    );
  },
);
