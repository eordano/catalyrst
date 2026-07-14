import { Component, type ReactNode } from "react";
import FatalErrorModal from "./FatalErrorModal";

type ErrorBoundaryProps = { children: ReactNode };
type ErrorBoundaryState = { error: Error | null };

export default class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  override state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  override componentDidCatch(error: Error): void {
    if (typeof console !== "undefined") {
      console.error("[react error boundary]", error);
    }
  }

  override render(): ReactNode {
    const { error } = this.state;
    if (error) {
      return (
        <FatalErrorModal
          message={error.stack ?? error.message}
          onReload={() => location.reload()}
        />
      );
    }
    return this.props.children;
  }
}
