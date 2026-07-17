import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { CodeBlock } from "./CodeBlock";

interface Props {
  text: string;
}

/// Renders assistant markdown — paragraphs, headers, lists, tables,
/// inline code, code blocks with syntax highlighting. Uses Travis's
/// existing typography.
export function MarkdownBody({ text }: Props) {
  return (
    <div className="text-bone-2 text-[14px] leading-relaxed prose-travis">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          // v0.28.72 — render <p> as <div>. react-markdown wraps
          // ambient inline text in <p>, and when the assistant
          // outputs a fenced code block inline with prose (which
          // Claude does constantly), the resulting <p><CodeBlock/></p>
          // is invalid HTML (<div>/<pre> can't nest in <p>). React 19
          // refuses to render the tree and CanvasErrorBoundary
          // catches the crash — which looked like "code is missing"
          // to the user. Using <div> keeps semantics identical for
          // styling and eats the nested block content without
          // complaint.
          p: ({ children }) => <div className="my-2">{children}</div>,
          h1: ({ children }) => (
            <h1 className="text-bone text-[20px] font-medium mt-4 mb-2">
              {children}
            </h1>
          ),
          h2: ({ children }) => (
            <h2 className="text-bone text-[16px] font-medium mt-3 mb-1.5">
              {children}
            </h2>
          ),
          h3: ({ children }) => (
            <h3 className="text-bone text-[14px] font-medium mt-2 mb-1">
              {children}
            </h3>
          ),
          strong: ({ children }) => (
            <strong className="text-bone font-medium">{children}</strong>
          ),
          em: ({ children }) => <em className="italic">{children}</em>,
          a: ({ href, children }) => (
            <a
              href={href}
              className="text-pulse-2 hover:underline"
              target="_blank"
              rel="noopener noreferrer"
            >
              {children}
            </a>
          ),
          ul: ({ children }) => (
            <ul className="my-2 space-y-1 list-disc list-outside ml-4">
              {children}
            </ul>
          ),
          ol: ({ children }) => (
            <ol className="my-2 space-y-1 list-decimal list-outside ml-4">
              {children}
            </ol>
          ),
          li: ({ children }) => <li className="leading-relaxed">{children}</li>,
          blockquote: ({ children }) => (
            <blockquote className="border-l-2 border-pulse/40 pl-3 my-2 text-bone-3 italic">
              {children}
            </blockquote>
          ),
          code: (props: {
            inline?: boolean;
            className?: string;
            children?: React.ReactNode;
          }) => {
            const { inline, className, children } = props;
            const text = String(children ?? "").replace(/\n$/, "");
            if (inline) {
              return (
                <code className="px-1 py-0.5 rounded bg-pulse/12 text-bone font-mono text-[12px]">
                  {children}
                </code>
              );
            }
            const langMatch = /language-(\w+)/.exec(className ?? "");
            const language = langMatch?.[1] ?? "text";
            return <CodeBlock code={text} language={language} />;
          },
          table: ({ children }) => (
            <div className="my-3 overflow-x-auto">
              <table className="w-full text-[12px] border-collapse">
                {children}
              </table>
            </div>
          ),
          thead: ({ children }) => (
            <thead className="border-b border-bone-3/20">{children}</thead>
          ),
          th: ({ children }) => (
            <th className="text-left text-bone-3 font-mono uppercase text-[10px] tracking-wider px-2 py-1">
              {children}
            </th>
          ),
          td: ({ children }) => (
            <td className="px-2 py-1 border-b border-bone-3/10 text-bone-2">
              {children}
            </td>
          ),
          hr: () => <hr className="my-4 border-bone-3/20" />,
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}
