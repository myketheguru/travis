/// Parses a chat reply for selection-UX markers and splits it into
/// text + chip segments. Travis (the LLM) emits markers per the L2E
/// prompt fragment; this turns them into clickable affordances.
///
/// Marker semantics:
///   ⊙ single-select option — click submits the option text
///   ⊕ add-new option       — click submits the option text (typically "Create new ...")
///   ⊡ multi-select option  — click toggles selection; submit-all via a separate action
///   📅 date prompt          — click opens a date picker; result submits ISO date string
///
/// Lines without a marker render as plain prose. Markers must appear at
/// the start of a line (after optional whitespace and an optional list
/// bullet like "- " or "* ").

export type ChipKind = "single" | "new" | "multi" | "date";

export type Chip = {
  kind: ChipKind;
  /// The text after the marker. This is what gets submitted when clicked.
  label: string;
};

export type ReplySegment =
  | { type: "text"; text: string }
  | { type: "chip"; chip: Chip };

const MARKER_TO_KIND: Record<string, ChipKind> = {
  "⊙": "single",
  "⊕": "new",
  "⊡": "multi",
  "📅": "date",
};

const MARKERS = Object.keys(MARKER_TO_KIND);

/// Strip a leading bullet ("- " / "* " / "> ") plus any whitespace, then
/// return [marker, rest] if the next character is one of our markers.
/// Returns null if not a chip line.
function parseChipLine(line: string): Chip | null {
  let s = line;
  // Strip leading whitespace.
  s = s.replace(/^\s+/, "");
  // Strip an optional markdown list bullet or blockquote marker.
  s = s.replace(/^([-*>]\s+)/, "");
  s = s.replace(/^\s+/, "");
  for (const m of MARKERS) {
    if (s.startsWith(m)) {
      const rest = s.slice(m.length).trim();
      if (!rest) return null;
      return { kind: MARKER_TO_KIND[m]!, label: rest };
    }
  }
  return null;
}

/// Split a reply into prose lines and chip lines. Adjacent text lines
/// stay joined as a single text segment so paragraph breaks render
/// naturally; chips become their own segments so the renderer can
/// stack them as buttons.
export function parseChatReply(reply: string): ReplySegment[] {
  const lines = reply.split(/\r?\n/);
  const out: ReplySegment[] = [];
  let textBuffer: string[] = [];

  const flushText = () => {
    if (textBuffer.length === 0) return;
    // Strip purely empty trailing lines from the buffer.
    while (textBuffer.length > 0 && textBuffer[textBuffer.length - 1]!.trim() === "") {
      textBuffer.pop();
    }
    if (textBuffer.length > 0) {
      out.push({ type: "text", text: textBuffer.join("\n") });
    }
    textBuffer = [];
  };

  for (const line of lines) {
    const chip = parseChipLine(line);
    if (chip) {
      flushText();
      out.push({ type: "chip", chip });
    } else {
      textBuffer.push(line);
    }
  }
  flushText();
  return out;
}

/// True iff the reply contains at least one chip — lets the renderer
/// decide to use the rich chip-aware path vs. a plain paragraph.
export const hasChips = (reply: string): boolean =>
  parseChatReply(reply).some((s) => s.type === "chip");
