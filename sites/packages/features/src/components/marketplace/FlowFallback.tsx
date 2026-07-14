import type { ReactNode } from "react";
import { Link } from "react-router";

import EmptyState from "@ui/components/EmptyState";
import MarketplaceChrome from "@ui/marketplace/frames/MarketplaceChrome";

export default function FlowFallback({
  title,
  subtitle,
  primaryHref = "/shop",
  primaryLabel = "Browse the shop",
  secondary,
}: {
  title: string;
  subtitle: string;
  primaryHref?: string;
  primaryLabel?: string;
  secondary?: ReactNode;
}) {
  return (
    <MarketplaceChrome active="collectibles" onTab={() => {}}>
      <EmptyState
        variant="screen"
        title={title}
        subtitle={subtitle}
        actions={
          <>
            <Link to={primaryHref} className="btn btn--primary btn--md">
              {primaryLabel}
            </Link>
            {secondary}
          </>
        }
      />
    </MarketplaceChrome>
  );
}
