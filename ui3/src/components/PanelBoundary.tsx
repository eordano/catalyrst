import { Component, Suspense, type ReactNode } from "react";
import Button from "../atoms/Button";
import ContentStatus from "./ContentStatus";
import "./panelboundary.css";

type Props = { children: ReactNode; label: string; onClose: () => void; standalone?: boolean };

export default class PanelBoundary extends Component<Props, { failed: boolean }> {
  override state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  override componentDidCatch(error: Error) { console.error("[panel]", error); }

  fallback(failed: boolean) {
    const { label, onClose, standalone } = this.props;
    return <div className={standalone ? "panel-boundary panel-boundary--standalone" : "panel-boundary"}>
      <ContentStatus pending={!failed} message={failed ? `Couldn't open ${label}. Please reload to try again.` : `Opening ${label}\u2026`} />
      {(standalone || failed) && <div className="panel-boundary__actions">
        <Button variant="secondary" onClick={onClose}>Close</Button>
        {failed && <Button onClick={() => window.location.reload()}>Reload</Button>}
      </div>}
    </div>;
  }

  override render() {
    return this.state.failed ? this.fallback(true)
      : <Suspense fallback={this.fallback(false)}>{this.props.children}</Suspense>;
  }
}
