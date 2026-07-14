import LeaseGatedActions from "./LeaseGatedActions";
import type { EscrowLease } from "@data/lib/catalyst/marketplace/escrow-lease";

export type LeasedAsset = {
  id: string;
  name: string;
  lease: EscrowLease;
};

export default function LeasedAssetsNotice({ items }: { items: LeasedAsset[] }) {
  if (items.length === 0) return null;
  return (
    <section className="lease-locked-assets" style={SECTION_STYLE}>
      <h2 style={{ color: "#fff", fontSize: 16, margin: "0 0 4px" }}>
        In escrow return window
      </h2>
      <p style={{ color: "rgba(255,255,255,0.7)", fontSize: 13, marginTop: 0 }}>
        These items were bought with Credits and are held in escrow during their
        15-day return window. You can sell, transfer, or list them once the
        window ends.
      </p>
      <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
        {items.map((it) => (
          <li key={it.id} style={ITEM_STYLE}>
            <div style={{ color: "#fff", marginBottom: 8 }}>{it.name}</div>
            <LeaseGatedActions item={null} lease={it.lease} />
          </li>
        ))}
      </ul>
    </section>
  );
}

const SECTION_STYLE: React.CSSProperties = {
  marginTop: 24,
  padding: 16,
  background: "rgba(255,255,255,0.04)",
  border: "1px solid rgba(255,255,255,0.12)",
  borderRadius: 12,
};
const ITEM_STYLE: React.CSSProperties = {
  padding: "12px 0",
  borderBottom: "1px solid rgba(255,255,255,0.1)",
};
