import { useState } from "react";
import { parseChatReply, type Chip } from "../lib/chatChips";

/// Renders a chat reply with selection-UX markers turned into clickable
/// chips. Text segments render as plain paragraphs preserving line
/// breaks; chip lines render as buttons that, when clicked, invoke
/// `onSubmit` with the chip's label as the user's next reply.
///
/// Multi-select chips accumulate locally and submit on demand via a
/// "Send selection" button.
export function ChatReplyBody({
  reply,
  onSubmit,
  disabled,
}: {
  reply: string;
  onSubmit: (text: string) => void;
  disabled?: boolean;
}) {
  const segments = parseChatReply(reply);
  const [multiSelections, setMultiSelections] = useState<Set<string>>(new Set());

  const toggleMulti = (label: string) => {
    setMultiSelections((prev) => {
      const next = new Set(prev);
      if (next.has(label)) {
        next.delete(label);
      } else {
        next.add(label);
      }
      return next;
    });
  };

  const submitMulti = () => {
    if (multiSelections.size === 0) return;
    const text = Array.from(multiSelections).join(", ");
    setMultiSelections(new Set());
    onSubmit(text);
  };

  const handleDate = (label: string) => {
    // Pop a transient date input. We use a hidden HTML input to leverage
    // the OS date picker rather than ship a library. The label becomes
    // the prefix Travis sees ("Period start: 2026-05-01").
    const input = document.createElement("input");
    input.type = "date";
    input.style.position = "fixed";
    input.style.left = "-9999px";
    document.body.appendChild(input);
    input.addEventListener(
      "change",
      () => {
        const value = input.value;
        document.body.removeChild(input);
        if (value) {
          onSubmit(`${label}: ${value}`);
        }
      },
      { once: true },
    );
    input.click();
  };

  const renderChip = (chip: Chip, key: number) => {
    if (chip.kind === "multi") {
      const selected = multiSelections.has(chip.label);
      return (
        <button
          key={key}
          disabled={disabled}
          onClick={() => toggleMulti(chip.label)}
          className={`text-left text-xs px-2.5 py-1.5 rounded-md border transition-colors disabled:opacity-50 ${
            selected
              ? "border-pulse/60 bg-pulse/[0.12] text-bone"
              : "border-pulse-2/30 bg-pulse-2/[0.04] text-bone-2 hover:border-pulse-2/60 hover:bg-pulse-2/[0.08]"
          }`}
        >
          <span className="mr-1.5 text-pulse-2 font-mono">{selected ? "▣" : "⊡"}</span>
          {chip.label}
        </button>
      );
    }
    if (chip.kind === "date") {
      return (
        <button
          key={key}
          disabled={disabled}
          onClick={() => handleDate(chip.label)}
          className="text-left text-xs px-2.5 py-1.5 rounded-md border border-pulse-2/30 bg-pulse-2/[0.04] text-bone-2 hover:border-pulse-2/60 hover:bg-pulse-2/[0.08] transition-colors disabled:opacity-50"
        >
          <span className="mr-1.5 text-pulse-2">📅</span>
          {chip.label}
        </button>
      );
    }
    // single / new
    const isNew = chip.kind === "new";
    return (
      <button
        key={key}
        disabled={disabled}
        onClick={() => onSubmit(chip.label)}
        className={`text-left text-xs px-2.5 py-1.5 rounded-md border transition-colors disabled:opacity-50 ${
          isNew
            ? "border-bone-3/40 bg-bone-3/[0.04] text-bone-2 hover:border-bone-2/60 hover:bg-bone-3/[0.08]"
            : "border-pulse-2/30 bg-pulse-2/[0.04] text-bone-2 hover:border-pulse-2/60 hover:bg-pulse-2/[0.08]"
        }`}
      >
        <span className={`mr-1.5 font-mono ${isNew ? "text-bone-3" : "text-pulse-2"}`}>
          {isNew ? "+" : "→"}
        </span>
        {chip.label}
      </button>
    );
  };

  // Group consecutive chips so they render as a row of buttons.
  const grouped: Array<{ type: "text"; text: string } | { type: "chips"; chips: Chip[] }> = [];
  for (const s of segments) {
    if (s.type === "chip") {
      const last = grouped[grouped.length - 1];
      if (last && last.type === "chips") {
        last.chips.push(s.chip);
      } else {
        grouped.push({ type: "chips", chips: [s.chip] });
      }
    } else {
      grouped.push(s);
    }
  }

  const hasMultiSelected = multiSelections.size > 0;

  return (
    <div className="flex flex-col gap-2">
      {grouped.map((g, i) =>
        g.type === "text" ? (
          <p
            key={i}
            className="text-bone leading-relaxed text-sm whitespace-pre-wrap"
          >
            {g.text}
          </p>
        ) : (
          <div key={i} className="flex flex-col gap-1.5" data-no-drag>
            {g.chips.map((c, j) => renderChip(c, j))}
          </div>
        ),
      )}
      {hasMultiSelected && (
        <div className="flex items-center gap-2 pt-1" data-no-drag>
          <button
            disabled={disabled}
            onClick={submitMulti}
            className="text-xs px-3 py-1.5 rounded-md bg-pulse/95 text-ink font-medium hover:bg-pulse transition-colors disabled:opacity-50"
          >
            Send selection ({multiSelections.size})
          </button>
          <button
            onClick={() => setMultiSelections(new Set())}
            className="text-bone-3 text-[11px] hover:text-bone-2"
          >
            clear
          </button>
        </div>
      )}
    </div>
  );
}
