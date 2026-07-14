import type { ComponentType, ReactNode } from "react";
import "./creatorhubbreadcrumb.css";

type LinkComponentProps = {
  className?: string;
  to: string;
  prefetch?: "intent" | "render" | "none" | "viewport";
  children?: ReactNode;
};

type CreatorHubBreadcrumbProps = {
  to: string;
  label?: string;
  LinkComponent?: ComponentType<LinkComponentProps>;
};

export default function CreatorHubBreadcrumb({
  to,
  label = "Back to scenes",
  LinkComponent,
}: CreatorHubBreadcrumbProps) {
  return (
    <nav aria-label="Breadcrumb" className="ch-breadcrumb">
      {LinkComponent ? (
        <LinkComponent className="ch-breadcrumb__link" to={to} prefetch="intent">
          &#x2039; {label}
        </LinkComponent>
      ) : (
        <a className="ch-breadcrumb__link" href={to}>
          &#x2039; {label}
        </a>
      )}
    </nav>
  );
}
