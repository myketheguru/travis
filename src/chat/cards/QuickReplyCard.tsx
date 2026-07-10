/**
 * QuickReplyCard — v0.28.28 Phase A.
 *
 * Pill options the user can click to answer instead of typing.
 * Click dispatches `travis:composer-submit` with the option's value
 * (or label) — Composer/AskTab picks it up as the next user turn.
 */
import { useAppStore } from "../../stores/app";
import type { QuickReplyOption } from "../../lib/richResponse";

interface Props {
  prompt?: string;
  options: QuickReplyOption[];
  narration?: string;
}

export function QuickReplyCard({ prompt, options, narration }: Props) {
  const setPendingComposerSubmit = useAppStore((s) => s.setPendingComposerSubmit);
  const setSpeakNextResponse = useAppStore((s) => s.setSpeakNextResponse);
  return (
    <div
      className="rounded-2xl px-4 py-3.5"
      style={{
        border: "1px solid rgba(189, 158, 255, 0.28)",
        background: "linear-gradient(180deg, rgba(28, 24, 40, 0.55), rgba(20, 18, 30, 0.52))",
      }}
    >
      {prompt && (
        <div className="text-[13px] mb-2.5 leading-snug" style={{ color: "rgba(236, 236, 241, 0.9)" }}>
          {prompt}
        </div>
      )}
      <div className="flex flex-wrap gap-2">
        {options.map((o) => (
          <button
            key={o.id}
            onClick={() => {
              // Click counts as a typed turn — silent reply.
              setSpeakNextResponse(false);
              setPendingComposerSubmit(o.value ?? o.label);
            }}
            className="px-3 py-1.5 rounded-full text-[12.5px] transition-all"
            style={{
              background: "rgba(189, 158, 255, 0.12)",
              border: "1px solid rgba(189, 158, 255, 0.45)",
              color: "rgba(236, 236, 241, 0.92)",
              cursor: "pointer",
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = "rgba(189, 158, 255, 0.22)";
              e.currentTarget.style.transform = "translateY(-1px)";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = "rgba(189, 158, 255, 0.12)";
              e.currentTarget.style.transform = "translateY(0)";
            }}
          >
            {o.label}
          </button>
        ))}
      </div>
      {narration && narration !== prompt && (
        <div className="mt-2.5 text-[12px]" style={{ color: "rgba(236, 236, 241, 0.65)" }}>
          {narration}
        </div>
      )}
    </div>
  );
}
