// Curated voice presets that map to a prompt-friendly description string.
// We persist the `description` (which is what gets injected into prompts),
// not the id — so the persisted value remains useful even if presets are
// later renamed or replaced.

export type VoicePreset = {
  id: string;
  label: string;
  blurb: string;
  /** Stored verbatim in user_profile.communication_style. */
  description: string;
};

export const VOICE_PRESETS: VoicePreset[] = [
  {
    id: "default",
    label: "Default",
    blurb: "No specific direction — Travis picks naturally.",
    description: "",
  },
  {
    id: "warm-direct",
    label: "Warm & direct",
    blurb: "Friendly but no fluff. Treats you like a sharp colleague.",
    description: "warm and direct, conversational, no fluff",
  },
  {
    id: "concise",
    label: "Concise",
    blurb: "Terse. Action first. The fewer words the better.",
    description: "terse and concise, action-first, minimal words",
  },
  {
    id: "formal",
    label: "Formal",
    blurb: "Professional, polished, always plain-prose.",
    description: "professional and polished, plain prose, no slang",
  },
  {
    id: "playful",
    label: "Playful",
    blurb: "Witty and light. Never sycophantic.",
    description: "witty and light, but never sycophantic, keeps it crisp",
  },
  {
    id: "coach",
    label: "Coach",
    blurb: "Encouraging, asks one good question per turn.",
    description:
      "encouraging without being pushy, asks one focused question per turn, surfaces what's worth thinking about",
  },
];

/** Map a stored description string back to a preset id (for round-tripping in UI). */
export function presetFromDescription(value: string | null | undefined): VoicePreset | null {
  const normalized = (value ?? "").trim();
  if (!normalized) {
    return VOICE_PRESETS.find((p) => p.id === "default") ?? null;
  }
  return VOICE_PRESETS.find((p) => p.description === normalized) ?? null;
}
