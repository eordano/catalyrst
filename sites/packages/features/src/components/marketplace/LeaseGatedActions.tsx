import {
  isInReturnWindow,
  readLease,
  returnWindowMessage,
  type EscrowLease,
  type LeasableItem,
} from "@data/lib/catalyst/marketplace/escrow-lease";

export type LeaseAction = {
  key: "sell" | "transfer" | "list";
  label: string;
  href?: string;
  onClick?: () => void;
};

export type LeaseGatedActionsProps = {
  item: LeasableItem | null | undefined;
  lease?: EscrowLease | null;
  actions?: LeaseAction[];
  now?: number;
};

const DEFAULT_ACTIONS: LeaseAction[] = [
  { key: "sell", label: "Sell" },
  { key: "transfer", label: "Transfer" },
  { key: "list", label: "List for sale" },
];

export default function LeaseGatedActions({
  item,
  lease,
  actions = DEFAULT_ACTIONS,
  now,
}: LeaseGatedActionsProps) {
  const resolved = lease ?? readLease(item);
  const locked = isInReturnWindow(resolved, now);

  return (
    <div className="lease-gated" data-locked={locked ? "true" : "false"}>
      <div className="lease-gated__row" style={ROW_STYLE}>
        {actions.map((a) => {
          const disabled = locked;
          if (a.href && !disabled) {
            return (
              <a key={a.key} href={a.href} style={LINK_STYLE}>
                <button type="button" style={BTN_STYLE}>
                  {a.label}
                </button>
              </a>
            );
          }
          return (
            <button
              key={a.key}
              type="button"
              disabled={disabled}
              onClick={disabled ? undefined : a.onClick}
              style={disabled ? BTN_DISABLED_STYLE : BTN_STYLE}
              aria-disabled={disabled}
              title={disabled && resolved ? returnWindowMessage(resolved) : undefined}
            >
              {a.label}
            </button>
          );
        })}
      </div>

      {locked && resolved && (
        <p className="lease-gated__message" role="status" style={MSG_STYLE}>
          {returnWindowMessage(resolved)}
        </p>
      )}
    </div>
  );
}

const ROW_STYLE: React.CSSProperties = { display: "flex", gap: 12, flexWrap: "wrap" };
const LINK_STYLE: React.CSSProperties = { textDecoration: "none" };
const BTN_STYLE: React.CSSProperties = {
  background: "#a855f7",
  color: "#fff",
  border: "none",
  borderRadius: 8,
  padding: "10px 18px",
  cursor: "pointer",
  fontWeight: 600,
};
const BTN_DISABLED_STYLE: React.CSSProperties = {
  ...BTN_STYLE,
  background: "rgba(255,255,255,0.12)",
  color: "rgba(255,255,255,0.5)",
  cursor: "not-allowed",
};
const MSG_STYLE: React.CSSProperties = {
  marginTop: 10,
  color: "#ffce6b",
  fontSize: 13,
};
