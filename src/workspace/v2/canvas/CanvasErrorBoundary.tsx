/**
 * CanvasErrorBoundary — v0.28.1.
 *
 * A crash in any of the canvas modes (MapCanvas parsing a malformed
 * response, ChatCanvas hitting a bad message, etc.) used to blank the
 * whole workspace because React lets errors propagate all the way up.
 * This boundary catches the render failure and shows a soft fallback
 * so the user gets at least the composer + chrome + a nudge to try
 * again — never a blank screen.
 */
import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  err: Error | null;
}

export class CanvasErrorBoundary extends Component<Props, State> {
  state: State = { err: null };

  static getDerivedStateFromError(err: Error): State {
    return { err };
  }

  componentDidCatch(err: Error, info: unknown) {
    // eslint-disable-next-line no-console
    console.error("[canvas] render error caught:", err, info);
  }

  handleReset = () => {
    this.setState({ err: null });
  };

  render() {
    if (this.state.err) {
      return (
        <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
          <div
            className="text-center max-w-md pointer-events-auto"
            style={{ color: "rgba(236, 236, 241, 0.75)" }}
          >
            <div
              className="text-[10px] uppercase tracking-[0.24em] font-mono mb-3"
              style={{ color: "rgba(255, 179, 92, 0.85)" }}
            >
              canvas hiccup
            </div>
            <div className="text-[15px] leading-relaxed">
              Something on the canvas failed to render. The composer still
              works — try asking again or press the button below to reset.
            </div>
            <button
              onClick={this.handleReset}
              className="mt-4 px-4 py-1.5 rounded-full text-[12px] tracking-wide"
              style={{
                background: "rgba(189, 158, 255, 0.14)",
                border: "1px solid rgba(189, 158, 255, 0.45)",
                color: "rgb(189, 158, 255)",
              }}
            >
              Reset canvas
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
