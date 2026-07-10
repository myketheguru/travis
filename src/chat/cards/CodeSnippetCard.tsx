/**
 * CodeSnippetCard — v0.28.28 Phase A.
 *
 * Syntax-highlighted code with copy button + optional filename
 * header. Uses prism-react-renderer (already a dependency).
 * Overflow scrolls horizontally inside the card.
 */
import { useState } from "react";
import { Highlight, themes } from "prism-react-renderer";

interface Props {
  code: string;
  language?: string;
  filename?: string;
  narration?: string;
}

export function CodeSnippetCard({ code, language, filename, narration }: Props) {
  const [copied, setCopied] = useState(false);
  const lang = (language ?? "text").toLowerCase();

  return (
    <div
      className="rounded-2xl overflow-hidden"
      style={{
        border: "1px solid rgba(189, 158, 255, 0.28)",
        background: "rgba(14, 12, 20, 0.85)",
      }}
    >
      <div
        className="flex items-center justify-between px-3.5 py-1.5"
        style={{ borderBottom: "1px solid rgba(189, 158, 255, 0.18)" }}
      >
        <div className="flex items-center gap-2 min-w-0">
          <span
            className="text-[10px] uppercase tracking-[0.22em] font-mono"
            style={{ color: "rgba(189, 158, 255, 0.85)" }}
          >
            {lang}
          </span>
          {filename && (
            <span
              className="text-[11px] truncate font-mono"
              style={{ color: "rgba(236, 236, 241, 0.65)" }}
            >
              {filename}
            </span>
          )}
        </div>
        <button
          onClick={async () => {
            try {
              await navigator.clipboard.writeText(code);
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1400);
            } catch {
              /* ignore */
            }
          }}
          className="text-[10px] uppercase tracking-wider font-mono px-2 py-0.5 rounded"
          style={{
            color: copied ? "rgb(140, 230, 175)" : "rgba(236, 236, 241, 0.65)",
            background: "transparent",
          }}
        >
          {copied ? "copied" : "copy"}
        </button>
      </div>
      <div className="overflow-x-auto">
        <Highlight code={code.replace(/\n$/, "")} language={lang} theme={themes.nightOwl}>
          {({ tokens, getLineProps, getTokenProps }) => (
            <pre
              className="text-[12.5px] leading-relaxed px-4 py-3 m-0"
              style={{ background: "transparent" }}
            >
              {tokens.map((line, i) => (
                <div key={i} {...getLineProps({ line })}>
                  {line.map((token, key) => (
                    <span key={key} {...getTokenProps({ token })} />
                  ))}
                </div>
              ))}
            </pre>
          )}
        </Highlight>
      </div>
      {narration && (
        <div
          className="px-4 py-2 text-[12px]"
          style={{ color: "rgba(236, 236, 241, 0.72)", borderTop: "1px solid rgba(189, 158, 255, 0.14)" }}
        >
          {narration}
        </div>
      )}
    </div>
  );
}
