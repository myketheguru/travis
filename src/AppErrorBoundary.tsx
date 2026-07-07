/**
 * AppErrorBoundary — v0.28.1.
 *
 * Top-level React error boundary. Anywhere below this — the whole
 * WorkspaceV2, all overlays, classic view, everything — a render
 * crash falls through to a soft fallback with an error stack + reset
 * button instead of a blank window. A render error should never
 * black-hole the UI.
 */
import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  err: Error | null;
  info: string | null;
}

export class AppErrorBoundary extends Component<Props, State> {
  state: State = { err: null, info: null };

  static getDerivedStateFromError(err: Error): State {
    return { err, info: null };
  }

  componentDidCatch(err: Error, info: { componentStack?: string | null }) {
    // eslint-disable-next-line no-console
    console.error("[app] render error:", err, info);
    this.setState({ info: info.componentStack ?? null });
  }

  handleReset = () => {
    this.setState({ err: null, info: null });
  };

  handleReload = () => {
    window.location.reload();
  };

  render() {
    if (this.state.err) {
      return (
        <div
          className="h-full w-full flex items-center justify-center p-8"
          style={{
            background:
              "radial-gradient(circle at 30% 20%, rgba(124, 92, 255, 0.10), transparent 60%), rgb(12, 12, 16)",
            color: "rgba(236, 236, 241, 0.85)",
          }}
        >
          <div className="max-w-lg w-full">
            <div
              className="text-[10px] uppercase tracking-[0.24em] font-mono mb-4"
              style={{ color: "rgba(255, 179, 92, 0.9)" }}
            >
              travis hit a snag
            </div>
            <div
              className="text-[19px] leading-relaxed mb-4"
              style={{ color: "rgb(236, 236, 241)" }}
            >
              A render error stopped the workspace from drawing. Your
              conversations are safe — this is just the UI.
            </div>
            <details className="mb-6">
              <summary
                className="cursor-pointer text-[12px] font-mono opacity-70 mb-2"
                style={{ color: "rgba(236, 236, 241, 0.65)" }}
              >
                Show technical details
              </summary>
              <pre
                className="text-[11px] leading-relaxed font-mono p-3 rounded-lg mt-2 overflow-x-auto"
                style={{
                  background: "rgba(0, 0, 0, 0.4)",
                  border: "1px solid rgba(255, 255, 255, 0.08)",
                  color: "rgba(255, 179, 92, 0.9)",
                }}
              >
                {this.state.err.message}
                {this.state.info ? `\n\n${this.state.info}` : ""}
              </pre>
            </details>
            <div className="flex gap-3">
              <button
                onClick={this.handleReset}
                className="px-4 py-2 rounded-full text-[13px]"
                style={{
                  background: "rgba(189, 158, 255, 0.14)",
                  border: "1px solid rgba(189, 158, 255, 0.50)",
                  color: "rgb(189, 158, 255)",
                }}
              >
                Try again
              </button>
              <button
                onClick={this.handleReload}
                className="px-4 py-2 rounded-full text-[13px]"
                style={{
                  background: "transparent",
                  border: "1px solid rgba(255, 255, 255, 0.20)",
                  color: "rgba(236, 236, 241, 0.75)",
                }}
              >
                Restart workspace
              </button>
            </div>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
