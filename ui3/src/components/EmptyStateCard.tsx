import EmptyState from "./EmptyState";
import type { EmptyStateProps } from "./EmptyState";

export default function EmptyStateCard({ className = "", ...rest }: EmptyStateProps) {
  return (
    <EmptyState
      className={"es--card" + (className ? " " + className : "")}
      {...rest}
    />
  );
}
