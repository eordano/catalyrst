import type { ReactNode } from "react";

import "./buywizard.css";

type MkBuyPageProps = {
  found?: boolean;
  children?: ReactNode;
};

export default function MkBuyPage({
  found = false,
  children = undefined,
}: MkBuyPageProps) {
  if (!found) {
    return (
      <main className="buy-route buy-route--empty" style={{ padding: 48, color: "#fff" }}>
        <h1>No item to buy</h1>
        <p style={{ opacity: 0.7 }}>
          We couldn&apos;t find a live listing for this item. Browse the{" "}
          <a href="/shop">marketplace</a> to pick something to buy.
        </p>
      </main>
    );
  }

  return (
    <main className="buy-route">
      <p className="buy-route__sim" role="note">
        Listing is live from Catalyst.
        Your wallet signs a <strong>real EIP-712</strong> executeOrder meta-transaction; the
        relayer <strong>submit is simulated</strong> (no spend) until the node&apos;s economy service is configured.
      </p>

      {children}
    </main>
  );
}
