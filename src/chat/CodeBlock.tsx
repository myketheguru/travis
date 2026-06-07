import { useState } from "react";
import { Highlight, themes } from "prism-react-renderer";

interface Props {
  code: string;
  language?: string;
  filename?: string;
  collapsible?: boolean;
  defaultCollapsed?: boolean;
}

/// Code block with syntax highlighting + copy button. Defaults to
/// expanded; can be initially collapsed for long code passages.
export function CodeBlock({
  code,
  language = "text",
  filename,
  collapsible = false,
  defaultCollapsed = false,
}: Props) {
  const [collapsed, setCollapsed] = useState(defaultCollapsed);
  const [copied, setCopied] = useState(false);

  const lineCount = code.split("\n").length;

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard rejected */
    }
  };

  return (
    <div
      className="my-2 rounded-md overflow-hidden text-[12px]"
      style={{
        background: "rgba(13, 15, 20, 0.85)",
        border: "1px solid rgba(124, 92, 255, 0.18)",
      }}
    >
      <div
        className="flex items-center gap-2 px-3 py-1.5 text-[10px] font-mono"
        style={{ background: "rgba(124, 92, 255, 0.08)" }}
      >
        <span className="text-bone-3">{filename ?? language}</span>
        <span className="text-bone-3/50">·</span>
        <span className="text-bone-3/70">
          {lineCount} line{lineCount === 1 ? "" : "s"}
        </span>
        <div className="ml-auto flex items-center gap-2">
          {collapsible && (
            <button
              onClick={() => setCollapsed((p) => !p)}
              className="text-bone-3 hover:text-bone-2"
            >
              {collapsed ? "expand" : "collapse"}
            </button>
          )}
          <button
            onClick={handleCopy}
            className="text-bone-3 hover:text-bone-2"
          >
            {copied ? "copied" : "copy"}
          </button>
        </div>
      </div>
      {!collapsed && (
        <Highlight
          theme={themes.vsDark}
          code={code.trimEnd()}
          language={language as never}
        >
          {({ tokens, getLineProps, getTokenProps }) => (
            <pre
              className="px-3 py-2 overflow-x-auto leading-relaxed"
              style={{ background: "transparent" }}
            >
              {tokens.map((line, i) => {
                const lineProps = getLineProps({ line });
                return (
                  <div key={i} {...lineProps}>
                    <span className="text-bone-3/40 font-mono pr-2 select-none">
                      {String(i + 1).padStart(2, " ")}
                    </span>
                    {line.map((token, j) => {
                      const tokenProps = getTokenProps({ token });
                      return <span key={j} {...tokenProps} />;
                    })}
                  </div>
                );
              })}
            </pre>
          )}
        </Highlight>
      )}
    </div>
  );
}
